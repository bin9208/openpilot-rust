"""Compare final-frame burn-in, grid and touch overlay pixels against source."""

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
results = []
for mode in ['burn-in', 'grid', 'touches']:
  images = []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{mode}.png'
    command = (
      [sys.executable, str(root / 'rust/tools/ui_qa/diagnostic_source.py'), mode, str(output)]
      if lane == 'source'
      else [str(args.binary), str(root), mode, str(output)]
    )
    with (args.output / f'{lane}-{mode}.log').open('w') as log:
      subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    images.append(np.asarray(Image.open(output)).astype(int))
  delta = abs(images[0] - images[1])
  result = {'mode': mode, 'different_pixels': int(np.any(delta != 0, axis=-1).sum()), 'max_channel_difference': int(delta.max())}
  results.append(result)
  print(json.dumps(result), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 for row in results), results
print('PASS: unchanged-source/native burn-in shader, grid and touch trail final-frame pixels')
