"""Actual settings widgets: injected touch frames, modal outcomes, Params and exact pixels."""

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]


def touch(frame, x, y, phase):
  return {
    'frame': frame,
    'events': [
      {'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': phase == 'press', 'released': phase == 'release', 'down': phase != 'release', 'time': frame / 20}
    ],
  }


def click(frame, x, y):
  return [touch(frame, x, y, 'press'), touch(frame + 1, x, y, 'release')]


def base(kind='toggles', big=True):
  return {
    'kind': kind,
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'rect': {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240},
    'frames': 80,
    'prime': -2,
    'params': {},
    'capture_effects': True,
    'steps': [],
  }


from openpilot.cereal import log


def raw_values():
  values = {}
  event = log.Event.new_message()
  calibration = event.init('liveCalibration')
  calibration.calStatus = 'calibrated'
  calibration.rpyCalib = [0, 0.027, -0.043]
  values['CalibrationParams'] = list(event.to_bytes())
  event = log.Event.new_message()
  event.init('liveDelay').calPerc = 73
  values['LiveDelay'] = list(event.to_bytes())
  event = log.Event.new_message()
  torque = event.init('liveTorqueParameters')
  torque.useParams = True
  torque.calPerc = 100
  values['LiveTorqueParameters'] = list(event.to_bytes())
  values['LiveParameters'] = [1, 2, 3]
  values['LiveParametersV2'] = [4, 5, 6]
  return values


cases = []
a = base('device')
a['params'] = {'DongleId': 'fixture-dongle', 'HardwareSerial': 'fixture-serial'}
cases.append(('device-identities', a))
a = base('device')
a['steps'] = click(10, 2040, 427) + click(20, 2040, 598) + click(30, 2040, 940)
cases.append(('device-open-pages', a))
a = base('device')
a['steps'] = [{'frame': 5, 'started': True}] + click(15, 2040, 598)
cases.append(('device-onroad-lock', a))
a = base('device')
a['raw_params'] = raw_values()
a['steps'] = [{'frame': 5, 'started': False}] + click(15, 150, 769)
cases.append(('device-calibration-description', a))
a = base('device')
a['raw_params'] = {'CalibrationParams': [1, 2], 'LiveDelay': [3], 'LiveTorqueParameters': [4]}
a['steps'] = [{'frame': 5, 'started': False}] + click(15, 150, 769)
cases.append(('device-malformed-calibration', a))
a = base('device')
a['raw_params'] = raw_values()
a['steps'] = click(10, 2040, 769) + [{'frame': 15, 'confirm': False}] + click(20, 2040, 769) + [{'frame': 25, 'confirm': True}]
cases.append(('device-reset-cancel-confirm', a))
a = base('device')
a['raw_params'] = raw_values()
a['steps'] = click(10, 2040, 769) + [{'frame': 15, 'engaged': True, 'confirm': True}] + click(20, 2040, 769)
cases.append(('device-reset-engaged-recheck', a))
for reboot in [True, False]:
  a = base('device')
  a['steps'] = [{'frame': 5, 'wheel': -100}] + click(15, 500 if reboot else 1600, 970) + [{'frame': 20, 'confirm': True}]
  cases.append(('device-reboot' if reboot else 'device-shutdown', a))
a = base('device')
a['steps'] = [{'frame': 5, 'wheel': -100}] + click(15, 500, 970) + [{'frame': 20, 'engaged': True, 'confirm': True}] + click(25, 1600, 970)
cases.append(('device-power-engaged-recheck', a))
a = base('device', False)
a['params'] = {'DongleId': 'fixture-dongle', 'HardwareSerial': 'fixture-serial'}
cases.append(('mici-device-identities', a))
for prime in [-2, 0, 2]:
  a = base('device', False)
  a['prime'] = prime
  a['params'] = {'DongleId': 'fixture-dongle'}
  a['steps'] = [{'frame': 15, 'scroll': 802}] + click(35, 200, 120)
  cases.append((f'mici-pair-{prime}', a))
for valid in [False, True]:
  a = base('device', False)
  a['time_valid'] = valid
  a['steps'] = [{'frame': 15, 'scroll': 802}] + click(35, 200, 120)
  cases.append((f'mici-pair-blocked-{valid}', a))
for operation, scroll, x in [('reset', 2912, 200), ('uninstall', 3334, 200), ('reboot', 3640, 180), ('shutdown', 3640, 420)]:
  a = base('device', False)
  a['raw_params'] = raw_values()
  a['steps'] = [{'frame': 15, 'scroll': scroll}] + click(35, x, 120) + [{'frame': 42, 'confirm': True}]
  cases.append((f'mici-{operation}', a))
a = base('device', False)
a['raw_params'] = raw_values()
a['steps'] = [{'frame': 15, 'scroll': 2912}] + click(35, 200, 120) + [{'frame': 42, 'engaged': True, 'confirm': True}] + click(50, 200, 120)
cases.append(('mici-reset-engaged-recheck', a))
a = base('device', False)
a['steps'] = [{'frame': 15, 'scroll': 3640}, {'frame': 35, 'ignition': True}]
cases.append(('mici-ignition-power-hidden', a))
for operation, params in [
  ('check', {'UpdaterState': 'idle'}),
  ('download', {'UpdaterFetchAvailable': '1'}),
  ('reboot', {'UpdateAvailable': '1'}),
  ('clock', {}),
]:
  a = base('device', False)
  a['params'] = params
  a['time_valid'] = operation != 'clock'
  a['steps'] = [{'frame': 15, 'scroll': 380}] + click(35, 200, 120)
  if operation == 'check':
    a['steps'] += [{'frame': 42, 'params': {'UpdaterState': 'checking...'}}, {'frame': 48, 'params': {'UpdaterState': 'idle'}}]
  cases.append((f'mici-updater-{operation}', a))
a = base('device', False)
a['frames'] = 310
a['params'] = {'UpdaterState': 'idle'}
a['steps'] = [{'frame': 15, 'scroll': 380}] + click(35, 200, 120)
cases.append(('mici-updater-timeout-recover', a))
a = base('device', False)
a['steps'] = [
  {'frame': 15, 'scroll': 380},
  {'frame': 30, 'params': {'UpdateFailedCount': '2'}},
  {'frame': 45, 'started': True},
  {'frame': 55, 'started': False, 'params': {'UpdateFailedCount': '0', 'UpdaterFetchAvailable': '1'}},
]
cases.append(('mici-updater-failure-onroad', a))
for big in [True, False]:
  a = base('regulatory', big)
  if big:
    a['steps'] = [{'frame': 15, 'wheel': -6}]
  else:
    a['steps'] = [
      touch(20, 350, 190, 'press'),
      touch(21, 350, 140, 'move'),
      touch(22, 350, 70, 'move'),
      touch(23, 350, 25, 'move'),
      touch(24, 350, 25, 'release'),
    ]
  cases.append(('regulatory-big' if big else 'regulatory-mici', a))
a = base('language')
cases.append(('language-list', a))
a = base('language')
a['steps'] = click(15, 350, 425) + click(25, 1600, 900)
cases.append(('language-select-de', a))
results = []
for language in ['en', 'ko']:
  for label, template in cases:
    scene = copy.deepcopy(template)
    scene['language'] = language
    name = f'{label}-{language}'
    path = args.output / f'{name}.json'
    path.write_text(json.dumps(scene))
    images = []
    states = []
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-ui148-', dir='/dev/shm') as namespace:
      env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
      for lane in ['source', 'native']:
        output = args.output / f'{lane}-{name}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        with (args.output / f'{lane}-{name}.log').open('w') as log:
          subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        images.append(np.asarray(Image.open(output)).astype(int))
        states.append(json.loads(output.with_suffix('.json').read_text()))
    for index, (source, native) in enumerate(zip(*states, strict=True)):
      assert source == native, (name, index, source, native)
    delta = abs(images[0] - images[1])
    row = {'scene': name, 'frames': len(states[0]), 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 for row in results), results
print(f'PASS: {len(results)} original/native settings scenarios with exact Params/effects/personality traces and final pixels')
