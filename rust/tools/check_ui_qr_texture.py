"""Original/native QR replacement and release through the real GL adapter."""

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
images, states = [], []
for lane in ['source', 'native']:
  output = args.output / f'{lane}.png'
  command = (
    [sys.executable, str(root / 'rust/tools/ui_application_qa/qr_source.py'), str(output)] if lane == 'source' else [str(args.binary), str(root), str(output)]
  )
  with (args.output / f'{lane}.log').open('w') as log:
    subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
  states.append(json.loads(output.with_suffix('.json').read_text()))
  images.append(np.asarray(Image.open(output)))
assert states[0] == states[1], states
assert np.array_equal(*images), 'QR render pixels differ'
(args.output / 'result.json').write_text(json.dumps({'states_exact': True, 'different_pixels': 0, 'late_native_texture_drop': 'process exited successfully'}))
print('PASS: actual source/native QR replace/reuse/empty/destroy and exact pixels; late native texture drop after renderer exit')
