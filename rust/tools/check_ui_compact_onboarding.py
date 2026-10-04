"""Complete compact onboarding on owned VisionIPC with source/native frame oracles."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from typing import TextIO
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--peer', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', default=':127')
parser.add_argument('--filter', default='')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def event(frame, x, y, kind):
  return {
    'frame': frame,
    'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': kind == 'press', 'released': kind == 'release', 'down': kind != 'release', 'time': frame / 20}],
  }


def confirm(start, page, item, x=268):
  return [
    {'frame': start, 'page': page, 'scroll_item': item},
    event(start + 2, x, 120, 'press'),
    event(start + 3, x, 120, 'release'),
    event(start + 28, 445, 120, 'press'),
    event(start + 29, 100, 120, 'move'),
    event(start + 30, 100, 120, 'release'),
  ]


def next_page(start, page, item):
  return [{'frame': start, 'page': page, 'scroll_item': item}, event(start + 2, 400, 120, 'press'), event(start + 3, 400, 120, 'release')]


def driver(rhd=False, detected=True, orientation=None):
  return {'rhd': rhd, 'detected': detected, 'orientation': orientation or [0, 0, 0], 'deviation': 0.1, 'eyes': [0.2, 0.8], 'glasses': 0.6}


def peer_read(peer: subprocess.Popen[str], reader: TextIO, expected: str) -> None:
  deadline = time.monotonic() + 15
  while True:
    line = reader.readline()
    if not line:
      assert peer.poll() is None, ('peer exited', peer.pid, peer.returncode, expected)
      assert time.monotonic() < deadline, ('peer response timeout', peer.pid, expected)
      time.sleep(0.001)
      continue
    if line.startswith(('Starting listener', 'Stopping listener')):
      continue
    assert line.strip() == expected, line
    return


cases = []
for record in [True, False]:
  steps = [
    {'frame': 0, 'driver': driver()},
    *confirm(20, 'terms', 4),
    *next_page(100, 'attention', 5),
    *next_page(135, 'pre-dm', 4),
    {'frame': 175, 'driver': driver(True)},
    {'frame': 190, 'driver': driver(False)},
    {'frame': 205, 'driver': driver(detected=False)},
    {'frame': 215, 'driver': driver()},
    {'frame': 230, 'awake': False, 'timeout': True},
    {'frame': 235, 'awake': True},
    event(360, 480, 190, 'press'),
    event(361, 480, 190, 'release'),
    *confirm(395, 'record-front', 2 if record else 3, 268 if record else 350),
  ]
  cases.append((f'complete-{record}', 460, steps, record))
help_steps = [dict(step, frame=step['frame'] + (120 if step['frame'] >= 360 else 0)) for step in cases[0][2]]
help_steps.extend(
  [event(280, 60, 190, 'press'), event(281, 60, 190, 'release'), event(310, 268, 10, 'press'), event(311, 268, 220, 'move'), event(312, 268, 220, 'release')]
)
cases.append(('help-back-complete', 580, sorted(help_steps, key=lambda step: step['frame']), True))
cases.append(('decline', 100, [{'frame': 0, 'driver': driver()}, *confirm(20, 'terms', 5, 350)], None))
cases.append(
  (
    'cached-lifecycle',
    40,
    [{'frame': 0, 'driver': driver()}, {'frame': 5, 'lifecycle': False}, {'frame': 10, 'lifecycle': True}, {'frame': 20, 'close': True}],
    None,
  )
)
results = []
for language in ['en', 'ko']:
  for label, frames, steps, record in cases:
    name = f'{label}-{language}'
    if args.filter and args.filter not in name:
      continue
    scene = {
      'kind': 'onboarding',
      'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
      'language': language,
      'prime': 0,
      'frames': frames,
      'steps': steps,
      'params': {'IsOnroad': '0', 'DriverTooDistracted': '1', 'RecordFront': '0' if record else '1'},
    }
    if label == 'cached-lifecycle':
      scene['params'].update(HasAcceptedTerms='2', CompletedTrainingVersion='0.2.0')
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    traces = []
    for lane in ['source', 'native']:
      with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-onboarding148-', dir='/dev/shm') as ns, tempfile.TemporaryDirectory() as sync:
        env = dict(
          os.environ,
          DISPLAY=args.display,
          PYTHONPATH=os.environ['UI_MSGQ_PYTHON'] + os.pathsep + str(root),
          OFFSCREEN='1',
          UI_CAMERA_SYNC=sync,
          OPENPILOT_PREFIX=Path(ns).name.removeprefix('msgq_'),
        )
        output = args.output / f'{lane}-{name}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/compact_onboarding_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        socket = Path('/tmp') / f'{env["OPENPILOT_PREFIX"]}_visionipc_rustvision'
        metadata = {
          'lane': lane, 'scene': name, 'command': [str(args.peer)], 'prefix': env['OPENPILOT_PREFIX'],
          'socket': str(socket), 'ready': False, 'stopped': False, 'forced_cleanup': False,
        }
        with output.with_suffix('.peer.stdout').open('w') as peer_out, output.with_suffix('.peer.stderr').open('w') as peer_err:
          with subprocess.Popen([str(args.peer)], env=env, stdin=subprocess.PIPE, stdout=peer_out, stderr=peer_err, text=True) as peer:
            metadata['pid'] = peer.pid
            output.with_suffix('.peer.json').write_text(json.dumps(metadata, indent=2))
            process = None
            try:
              assert peer.stdin is not None
              with output.with_suffix('.peer.stdout').open() as reader:
                peer_read(peer, reader, 'READY')
                metadata['ready'] = True
                with output.with_suffix('.log').open('w') as log:
                  process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
                  for frame in range(frames):
                    deadline = time.monotonic() + 15
                    while not (Path(sync) / f'{frame}.ready').exists():
                      assert process.poll() is None, output.with_suffix('.log').read_text()[-4000:]
                      assert time.monotonic() < deadline, (lane, name, frame)
                      time.sleep(0.001)
                    peer.stdin.write(f'send {frame + 32}\n')
                    peer.stdin.flush()
                    peer_read(peer, reader, 'OK')
                    (Path(sync) / f'{frame}.allow').write_text('allow')
                  assert process.wait(timeout=15) == 0, output.with_suffix('.log').read_text()[-4000:]
                traces.append(json.loads(output.with_suffix('.json').read_text()))
                peer.stdin.write('stop\n')
                peer.stdin.flush()
                peer_read(peer, reader, 'OK')
                assert peer.wait(timeout=5) == 0
                metadata['stopped'] = True
                assert not socket.exists(), socket
            finally:
              if process is not None and process.poll() is None:
                process.kill()
                process.wait()
              if peer.poll() is None:
                try:
                  peer.communicate(input='stop\n', timeout=5)
                except (OSError, subprocess.TimeoutExpired) as error:
                  metadata['cleanup_error'] = str(error)
                  metadata['forced_cleanup'] = True
                  peer.kill()
                  peer.wait()
              metadata.update(returncode=peer.returncode, socket_after_peer_exit=socket.exists())
              socket.unlink(missing_ok=True)
              metadata['socket_remaining'] = socket.exists()
              output.with_suffix('.peer.json').write_text(json.dumps(metadata, indent=2))
    differences = []
    states = []
    for index, (source, native) in enumerate(zip(*traces, strict=True)):
      if abs(source['progress'] - native['progress']) > 1e-11 or {k: v for k, v in source.items() if k != 'progress'} != {
        k: v for k, v in native.items() if k != 'progress'
      }:
        states.append({'frame': index, 'source': source, 'native': native})
      images = [np.asarray(Image.open(args.output / f'{lane}-{name}-{index}.png')) for lane in ['source', 'native']]
      delta = np.any(images[0] != images[1], axis=-1)
      if delta.any():
        differences.append({'frame': index, 'pixels': int(delta.sum())})
    result = {'scene': name, 'frames': frames, 'state_differences': states, 'pixel_differences': differences}
    results.append(result)
    (args.output / 'results.json').write_text(json.dumps(results, indent=2))
    print(json.dumps(result), flush=True)
    if record is not None:
      assert traces[0][-1]['effects'] == ['confirm', 'confirm', 'completed'] and traces[0][-1]['record_front'] == record, traces[0][-1]
      assert traces[0][-1]['accepted'] and traces[0][-1]['trained'] and not traces[0][-1]['driver_view'], traces[0][-1]
      assert any(row['frame'] and row['good'] for row in traces[0]), name
    elif label == 'decline':
      assert traces[0][-1]['uninstall'], traces[0][-1]
    else:
      assert traces[0][-1]['completed'] and traces[0][-1]['effects'] == ['completed'], traces[0][-1]
      assert traces[0][5]['timeout'] is None and traces[0][5]['brightness'] == 65
      assert traces[0][10]['timeout'] == 300 and traces[0][10]['brightness'] == 100
assert results and all(not row['state_differences'] and not row['pixel_differences'] for row in results), results
print('PASS full compact onboarding, owned camera frames, Params and navigation')
