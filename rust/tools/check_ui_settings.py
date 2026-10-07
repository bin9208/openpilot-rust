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


def car(alpha=False, long=True):
  return {'alpha_longitudinal_available': alpha, 'openpilot_longitudinal_control': long, 'max_lateral_accel': 2.2}


cases = []
a = base()
a['car'] = car()
a['steps'] = click(10, 2040, 255) + [{'frame': 14, 'confirm': False}] + click(20, 2040, 255) + [{'frame': 24, 'confirm': True}] + click(32, 2040, 255)
cases.append(('experimental-confirm-cancel', a))
a = base()
a['steps'] = (
  click(10, 2040, 85)
  + [{'frame': 16, 'engaged': True}]
  + click(20, 2040, 85)
  + [{'frame': 28, 'engaged': False, 'params': {'OpenpilotEnabledToggle': '0'}}]
  + click(32, 2040, 85)
)
cases.append(('restart-engaged-lock', a))
a = base()
a['car'] = car(long=False)
a['params'] = {'ExperimentalMode': '1'}
a['steps'] = click(10, 200, 255) + click(20, 2040, 255)
cases.append(('stock-acc-description', a))
a = base()
a['car'] = car(alpha=True, long=False)
a['params'] = {'IsReleaseBranch': '1', 'ExperimentalMode': '1'}
a['steps'] = click(10, 200, 255)
cases.append(('alpha-release-description', a))
a = base()
a['car'] = car(alpha=True, long=False)
a['steps'] = [{'frame': 8, 'engaged': False, 'params': {'AlphaLongitudinalEnabled': '1'}}] + click(15, 2040, 255) + [{'frame': 19, 'confirm': True}]
cases.append(('alpha-enable', a))
a = base()
a['params'] = {'LongitudinalPersonality': '1'}
a['steps'] = click(10, 1757, 595) + [{'frame': 18, 'started': True, 'personality': 3}, {'frame': 25, 'started': False, 'personality': 1}]
cases.append(('personality-sync', a))
a = base(big=False)
a['params'] = {'LongitudinalPersonality': '-1'}
a['steps'] = click(25, 200, 120) + [{'frame': 35, 'personality': 2}, {'frame': 45, 'started': True, 'personality': 2}, {'frame': 55, 'personality': 3}]
cases.append(('mici-personality-index', a))
a = base(big=False)
a['params'] = {'RecordFrontLock': '1', 'ShowDebugInfo': '1'}
a['steps'] = (
  [{'frame': 15, 'scroll': 2000}]
  + click(30, 300, 120)
  + [{'frame': 38, 'scroll': 422}, {'frame': 45, 'engaged': True}]
  + click(50, 300, 120)
  + [{'frame': 60, 'engaged': False}]
  + click(66, 300, 120)
)
cases.append(('mici-record-locks', a))
for big in [True, False]:
  a = base('firehose', big)
  a['network_type'] = 1
  a['params'] = {'ApiCache_FirehoseStats': '{"firehose":42}'}
  if big:
    a['steps'] = [{'frame': 18, 'wheel': -6}]
  else:
    a['steps'] = [
      touch(20, 350, 190, 'press'),
      touch(21, 350, 140, 'move'),
      touch(22, 350, 85, 'move'),
      touch(23, 350, 25, 'move'),
      touch(24, 350, 25, 'release'),
    ]
  cases.append(('firehose-big-scroll' if big else 'firehose-mici-scroll', a))
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
