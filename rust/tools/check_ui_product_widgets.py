"""Strict original/native product widgets in both languages and viewport sizes."""

import argparse
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
results = []
cases = [
  ('prime-free', 'prime', 0, None),
  ('prime-paid', 'prime', 1, None),
  ('setup-pair', 'setup', -2, None),
  ('setup-firehose', 'setup', 0, None),
  ('web', 'carrot-web', 0, '192.0.2.4'),
  ('web-ipv6', 'carrot-web', 0, '2001:db8:1234:5678::abcd'),
  ('web-offline', 'carrot-web', 0, None),
]
for language in ['en', 'ko']:
  for name, kind, prime, address in cases:
    big = kind != 'carrot-web' or name == 'web-ipv6'
    width, height = (2160, 1080) if big else (536, 240)
    scene = {
      'kind': kind,
      'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
      'language': language,
      'rect': {'x': 0, 'y': 0, 'width': 750 if kind != 'carrot-web' else width, 'height': 900 if kind != 'carrot-web' else height},
      'frames': 3,
      'prime': prime,
      'address': address,
      'params': {},
    }
    case = f'{name}-{language}'
    path = args.output / f'{case}.json'
    path.write_text(json.dumps(scene))
    images = []
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-ui148-', dir='/dev/shm') as namespace:
      env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(namespace).name.removeprefix('msgq_'))
      for lane in ['source', 'native']:
        output = args.output / f'{lane}-{case}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/product_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        with (args.output / f'{lane}-{case}.log').open('w') as log:
          subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        images.append(np.asarray(Image.open(output)).astype(int))
    delta = abs(images[0] - images[1])
    row = {'scene': case, 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())}
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 for row in results), results
print('PASS: 14 source/native Prime, setup and Carrot Web QR screens in English/Korean with exact pixels')
