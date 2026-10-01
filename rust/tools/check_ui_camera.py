import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--peer', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
parser.add_argument('--filter', default='')
parser.add_argument('--driver', action='store_true')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def peer_read(peer, expected):
  while True:
    line = peer.stdout.readline()
    assert line, peer.poll()
    if line.startswith(('Starting listener', 'Stopping listener')):
      continue
    assert line.strip() == expected, line
    return


results = []
for big in [False] if args.driver else [True, False]:
  for name, stream, transform, engaged, switches in [
    ('road', 0, 0, False, []),
    ('driver', 1, 0, False, []),
    ('driver-zoom', 1, 1 if big else 2, False, []),
    ('matrix-engaged', 2, 3, True, []),
    ('switch', 0, 0, False, [(10, 1), (16, 2)]),
    ('lifecycle', 0, 0, False, []),
  ]:
    if args.driver and name == 'lifecycle':
      continue
    label = ('big-' if big else 'mici-') + name
    if args.filter and args.filter not in label:
      continue
    scene = {
      'kind': 'camera',
      'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
      'language': 'en',
      'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
      'frames': 24,
      'prime': 0,
      'params': {},
      'steps': [],
      'capture_frames': list(range(24)),
      'camera': {'stream': stream, 'transform': transform, 'engaged': engaged, 'switches': switches},
    }
    if name == 'lifecycle':
      scene['steps'] = [{'frame': 14, 'started': True}, {'frame': 18, 'started': False}]
    if args.driver:
      scene['driver'] = {
        'setup': name in ['road', 'driver-zoom'],
        'rhd': name in ['driver', 'matrix-engaged'],
        'detected': name != 'switch',
        'orientation': [0.0, 0.0, 0.0] if name in ['road', 'driver'] else [0.1, -0.3, 0.0],
        'deviation': 0.1 if name != 'matrix-engaged' else 0.4,
        'eyes': [0.2, 0.8],
        'glasses': 0.6,
      }
      scene['camera']['switches'] = []
      if name == 'matrix-engaged':
        scene['rect'] = {'x': 32.3, 'y': 4.7, 'width': 486.4, 'height': 227.3}
      if name == 'driver-zoom':
        scene['rect'] = {'x': -12.7, 'y': 9.1, 'width': 550.2, 'height': 225.1}
      scene['steps'] = [{'frame': 22, 'params': {'IsOnroad': '1'}}]
      scene['params'] = {'IsOnroad': '0', 'DriverTooDistracted': '1'}
    path = args.output / f'{label}.json'
    path.write_text(json.dumps(scene))
    traces = []
    for lane in ['source', 'native']:
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-camera148-', dir='/dev/shm') as namespace, tempfile.TemporaryDirectory() as sync:
        env = dict(
          os.environ,
          DISPLAY=args.display,
          PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
          OFFSCREEN='1',
          UI_CAMERA_SYNC=sync,
          OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'),
        )
        peer = subprocess.Popen([str(args.peer)], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        peer_read(peer, 'READY')
        recorder = None
        recording = args.output / f'{lane}-{label}-messages.json'
        recording.with_suffix('.stop').unlink(missing_ok=True)
        if args.driver:
          recorder = subprocess.Popen(
            [sys.executable, str(root / 'rust/tools/ui_application_qa/driver_recorder.py'), str(recording)], env=env, stdout=subprocess.PIPE, text=True
          )
          assert recorder.stdout.readline().strip() == 'READY'
        output = args.output / f'{lane}-{label}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        try:
          with output.with_suffix('.log').open('w') as log:
            process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
            for frame in range(scene['frames']):
              deadline = time.monotonic() + 15
              while not (Path(sync) / f'{frame}.ready').exists():
                assert process.poll() is None, output.with_suffix('.log').read_text()[-3000:]
                assert time.monotonic() < deadline, (label, lane, frame)
                time.sleep(0.001)
              if name == 'lifecycle' and frame == 11:
                peer.stdin.write('restart\n')
                peer.stdin.flush()
                peer_read(peer, 'OK')
              if not (name == 'lifecycle' and 6 <= frame <= 9):
                peer.stdin.write(f'send {frame + 32}\n')
                peer.stdin.flush()
                peer_read(peer, 'OK')
              (Path(sync) / f'{frame}.allow').write_text('allow')
            assert process.wait(timeout=15) == 0, output.with_suffix('.log').read_text()[-3000:]
          traces.append(json.loads(output.with_suffix('.json').read_text()))
          if recorder is not None:
            recording.with_suffix('.stop').write_text('stop')
            assert recorder.wait(timeout=5) == 0
        finally:
          if process.poll() is None:
            process.kill()
            process.wait()
          peer.kill()
          peer.wait()
          if recorder is not None and recorder.poll() is None:
            recorder.kill()
            recorder.wait()
    if args.driver:
      messages = [json.loads((args.output / f'{lane}-{label}-messages.json').read_text()) for lane in ['source', 'native']]
      stable = [[row for row in rows if 0 < row['time'] <= 1_000_000_000] for rows in messages]
      expected = [] if scene['driver']['setup'] else [{'valid': False, 'time': int(index / 20 * 1e9), 'sound': 'none'} for index in range(1, 21)]
      assert stable[0] == stable[1] == expected, (label, messages)
      assert all(row['time'] < 1_100_000_000 for rows in messages for row in rows), (label, messages)

    assert traces[0] == traces[1], (label, traces)
    assert any(row['camera']['frame'] for row in traces[0]), label
    if name == 'lifecycle':
      assert all(row['camera']['frame'] == traces[0][5]['camera']['frame'] for row in traces[0][6:10])
      assert traces[0][-1]['camera']['frame'] == 55
    if args.driver:
      assert traces[0][20]['camera']['enabled'] is False and traces[0][20]['camera']['timeout'] is None
      assert traces[0][21]['camera']['enabled'] is True and traces[0][21]['camera']['timeout'] == 300
    differences = []
    for frame in scene['capture_frames']:
      images = [np.asarray(Image.open(args.output / f'{lane}-{label}-frame-{frame:04}.png')).astype(int) for lane in ['source', 'native']]
      delta = abs(images[0] - images[1])
      if delta.any():
        differences.append({'frame': frame, 'pixels': int(np.any(delta, axis=-1).sum()), 'max_channel': int(delta.max())})
    result = {'scene': label, 'frames': len(traces[0]), 'different_frames': differences}
    results.append(result)
    print(json.dumps(result), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert results and all(not result['different_frames'] for result in results), results
print('PASS: owned original VisionIPC server; exact source/native camera frames and metadata')
