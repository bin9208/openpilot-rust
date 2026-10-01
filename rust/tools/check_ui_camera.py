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
parser.add_argument('--large', action='store_true')
parser.add_argument('--navigation', action='store_true')
parser.add_argument('--visibility', action='store_true')
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
for big in ([args.large] if args.driver else [True, False]):
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
        'setup': not big and name in ['road', 'driver-zoom'],
        'rhd': name in ['driver', 'matrix-engaged'],
        'detected': name != 'switch',
        'orientation': [0.0, 0.0, 0.0] if name in ['road', 'driver'] else [0.1, -0.3, 0.0],
        'deviation': 0.1 if name != 'matrix-engaged' else 0.4,
        'eyes': [0.2, 0.8],
        'glasses': 0.6,
      }
      scene['camera']['switches'] = []
      if args.visibility:
        assert big
        scene['driver']['visibility_steps'] = [
          {'frame':0,'rhd':True,'started_frame':1000000,'alert_size':0},
          {'frame':6,'rhd':True,'started_frame':0,'alert_size':0},
          {'frame':12,'rhd':False,'started_frame':0,'alert_size':3},
          {'frame':18,'rhd':False,'started_frame':0,'alert_size':0},
        ]
      if big:
        scene['capture_effects'] = True
        scene['driver']['timeout'] = 19 if name == 'driver' else 0
      if not big and name == 'matrix-engaged':
        scene['rect'] = {'x': 32.3, 'y': 4.7, 'width': 486.4, 'height': 227.3}
      if not big and name == 'driver-zoom':
        scene['rect'] = {'x': -12.7, 'y': 9.1, 'width': 550.2, 'height': 225.1}
      scene['steps'] = [{'frame': 22, 'params': {'IsOnroad': '1'}}]
      scene['params'] = {'IsOnroad': '0', 'DriverTooDistracted': '1'}
      if big and name == 'switch':
        scene['steps'] += [{'frame':19+i,'events':[{'pos':{'x':500,'y':500},'slot':0,'pressed':i==0,'released':i==1,'down':i==0,'time':(19+i)/20}]}
                           for i in range(2)]
      if args.navigation:
        assert not big
        scene['driver'].update(navigation=True,setup=False,timeout=19 if name=='driver' else 0)
        scene['capture_effects']=True
        scene['frames']=60
        scene['capture_frames']=list(range(60))
        if name=='switch':
          scene['steps'] += [{'frame':14+i,'events':[{'pos':{'x':180,'y':y},'slot':0,'pressed':i==0,'released':i==3,'down':i!=3,'time':(14+i)/20}]}
                             for i,y in enumerate([20,80,200,200])]
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
        if args.driver and not big:
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
    if args.driver and not big:
      messages = [json.loads((args.output / f'{lane}-{label}-messages.json').read_text()) for lane in ['source', 'native']]
      stable = [[row for row in rows if 0 < row['time'] <= 1_000_000_000] for rows in messages]
      expected = [] if scene['driver']['setup'] else [{'valid': False, 'time': int(index / 20 * 1e9), 'sound': 'none'} for index in range(1, 21)]
      assert stable[0] == stable[1] == expected, (label, messages)
      assert all(row['time'] < 1_100_000_000 for rows in messages for row in rows), (label, messages)

    assert traces[0] == traces[1], next((label,index,a,b) for index,(a,b) in enumerate(zip(*traces,strict=True)) if a!=b)
    assert any(row['camera']['frame'] for row in traces[0]), label
    if name == 'lifecycle':
      assert all(row['camera']['frame'] == traces[0][5]['camera']['frame'] for row in traces[0][6:10])
      assert traces[0][-1]['camera']['frame'] == 55
    if args.driver and big:
      assert traces[0][-1]['camera']['enabled'] is False and traces[0][-1]['camera']['frame'] is False
      if name in ['driver','switch']:
        assert {'pop': True} in traces[0][-1]['effects']
    if args.driver and not big and not args.navigation:
      assert traces[0][20]['camera']['enabled'] is False and traces[0][20]['camera']['timeout'] is None
      assert traces[0][21]['camera']['enabled'] is True and traces[0][21]['camera']['timeout'] == 300
    if args.driver:
      lifecycle=json.loads((args.output / f'native-{label}.driver-lifecycle.json').read_text())
      assert lifecycle['remaining_callbacks']==0,(label,lifecycle)
    if args.navigation:
      assert traces[0][-1]['camera']['enabled'] is False and traces[0][-1]['camera']['timeout'] is None
      if name in ['driver','switch']:
        assert {'pop':True} in traces[0][-1]['effects'],(label,traces[0][-1])
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
