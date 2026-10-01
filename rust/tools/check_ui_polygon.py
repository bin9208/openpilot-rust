"""Actual shader-source versus native raster and triangulation oracle."""

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
env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1', PYTHONPATH=str(root))
points = [{'x': 30, 'y': 220}, {'x': 100, 'y': 160}, {'x': 130, 'y': 70}, {'x': 280, 'y': 20}, {'x': 420, 'y': 70}, {'x': 450, 'y': 160}, {'x': 506, 'y': 220}]
base = {
  'points': points,
  'origin': {'x': 20, 'y': 10, 'width': 496, 'height': 220},
  'mode': 'gradient',
  'start': [0, 1],
  'end': [0, 0],
  'colors': [0xFF0000FF, 0xFF00FF00, 0xFFFF0000],
  'stops': [],
}
scenes = {
  'odd-gradient': base,
  'solid': {**base, 'mode': 'solid'},
  'color': {**base, 'mode': 'color'},
  'clipped-stops': {**base, 'stops': [-2, 0.3, 2]},
  'empty-gradient': {**base, 'colors': [], 'expect_error': True},
  'capped-gradient': {**base, 'colors': [0x8080FF00 + i for i in range(24)], 'stops': []},
  'empty': {**base, 'points': [{'x': 0, 'y': 0}]},
}
results = []
for name, scene in scenes.items():
  path = args.output / f'{name}.json'
  path.write_text(json.dumps(scene))
  images = []
  states = []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{name}.png'
    command = (
      [sys.executable, str(root / 'rust/tools/ui_qa/polygon_source.py'), str(path), str(output)]
      if lane == 'source'
      else [str(args.binary), str(root), str(path), str(output)]
    )
    with (args.output / f'{lane}-{name}.log').open('w') as log:
      subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    images.append(np.asarray(Image.open(output)).astype(int))
    states.append(json.loads(output.with_suffix('.json').read_text()))
  delta = abs(images[0] - images[1])
  result = {
    'scene': name,
    'different_pixels': int(np.any(delta != 0, axis=-1).sum()),
    'max_channel_difference': int(delta.max()),
    'state_equal': states[0] == states[1],
  }
  results.append(result)
  print(json.dumps(result), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 and row['state_equal'] for row in results), results
print('PASS: 7 actual-source/native polygon shader, gradient and triangulation scenes')
