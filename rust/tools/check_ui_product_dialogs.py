"""Original/native compact dialogs: actual input, dismissal and callback outcomes."""

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


def event(frame, x, y, kind):
  return {
    'frame': frame,
    'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': kind == 'press', 'released': kind == 'release', 'down': kind != 'release', 'time': frame / 20}],
  }


cases = [
  ('information', 'dialog-info', {}, []),
  ('confirm-normal', 'dialog-confirm', {}, []),
  ('confirm-red', 'dialog-confirm', {'red': True}, []),
  ('confirm-slide', 'dialog-confirm', {}, [event(20, 445, 120, 'press'), event(21, 100, 120, 'move'), event(22, 100, 120, 'release')]),
  ('confirm-stay', 'dialog-confirm', {'stay': True}, [event(20, 445, 120, 'press'), event(21, 100, 120, 'move'), event(22, 100, 120, 'release')]),
  ('input-empty', 'dialog-input', {}, []),
  ('input-long', 'dialog-input', {'text': 'This is a longer value that must scroll left'}, []),
  ('input-backspace', 'dialog-input', {'text': 'abcdefghi'}, [event(20, 490, 30, 'press'), event(38, 490, 30, 'release')]),
  ('input-confirm', 'dialog-input', {'text': 'fixture'}, [event(20, 40, 30, 'press'), event(21, 40, 30, 'release')]),
  ('input-key', 'dialog-input', {}, [event(20, 165, 105, 'press'), event(21, 165, 105, 'release')]),
]
results = []
for language in ['en', 'ko']:
  for name, kind, options, steps in cases:
    scene = {
      'kind': kind,
      'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0},
      'language': language,
      'rect': {'x': 0, 'y': 0, 'width': 536, 'height': 240},
      'frames': 60,
      'prime': -2,
      'params': {},
      'steps': steps,
      'dialog': {
        'title': 'Enter text' if language == 'en' else '텍스트 입력',
        'description': 'Dialog description' if language == 'en' else '대화 상자 설명',
        **options,
      },
    }
    case = f'{name}-{language}'
    path = args.output / f'{case}.json'
    path.write_text(json.dumps(scene))
    images, traces = [], []
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
        traces.append(json.loads(output.with_suffix('.json').read_text()))
    delta = abs(images[0] - images[1])
    row = {
      'scene': case,
      'different_pixels': int(np.any(delta != 0, axis=-1).sum()),
      'max_channel_difference': int(delta.max()),
      'trace_equal': traces[0] == traces[1],
      'callbacks': traces[0][-1]['callbacks'],
    }
    results.append(row)
    print(json.dumps(row), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(row['different_pixels'] == 0 and row['trace_equal'] for row in results), results
print('PASS: all compact dialog images and callback traces match')
