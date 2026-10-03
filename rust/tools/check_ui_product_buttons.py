"""Strict source/native Mici button state and final-frame pixel gate."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1')


def touch(frame, pressed):
  return {
    'frame': frame,
    'events': [{'pos': {'x': 268, 'y': 110}, 'slot': 0, 'pressed': pressed, 'released': not pressed, 'down': pressed, 'time': frame / 20}],
  }


results = []
for kind in ['circle', 'circle-red', 'circle-toggle', 'button', 'toggle', 'multiple', 'grey', 'scroll']:
  actions = [
    touch(3, True),
    touch(5, False),
    {'frame': 8, 'operation': 'disable'},
    touch(9, True),
    touch(10, False),
    {'frame': 12, 'operation': 'enable'},
    touch(15, True),
    touch(17, False),
  ]
  if kind in ['button', 'scroll']:
    actions += [{'frame': 20, 'operation': 'grow'}, {'frame': 25, 'operation': 'shake'}, {'frame': 24, 'operation': 'rotate'}]
  scene = {
    'kind': kind,
    'text': 'Settings and connectivity' if kind == 'scroll' else 'Product settings',
    'value': 'Network connected',
    'language': 'en',
    'frames': 35,
    'actions': actions,
  }
  path = args.output / f'{kind}.json'
  path.write_text(json.dumps(scene))
  images = []
  states = []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{kind}.png'
    cmd = (
      [sys.executable, str(root / 'rust/tools/ui_application_qa/mici_buttons_source.py'), str(path), str(output)]
      if lane == 'source'
      else [str(args.binary), str(root), str(path), str(output)]
    )
    with (args.output / f'{lane}-{kind}.log').open('w') as log:
      subprocess.run(cmd, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    images.append(np.asarray(Image.open(output)).astype(int))
    states.append(json.loads(output.with_suffix('.json').read_text()))
  for index, (source, native) in enumerate(zip(*states, strict=True)):
    assert source['calls'] == native['calls'], (kind, index, source, native)
    for key, value in source['state'].items():
      actual = native['state'][key]
      assert abs(value - actual) < 1e-11 if isinstance(value, float) else value == actual, (kind, index, key, value, actual)
  delta = abs(images[0] - images[1])
  row = {'kind': kind, 'frames': scene['frames'], 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())}
  results.append(row)
  print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 for row in results), results
print('PASS: 8 actual-source/native product button scenes, 280 interaction/animation frames and exact pixels')
