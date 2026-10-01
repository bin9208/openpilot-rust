"""Reproducible source/native shared UI screenshots on a caller-owned X server."""

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
env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1', PYTHONPATH=str(root), PARAMS_ROOT=str(args.output / 'params'))


def element(kind, text, x, y, w, h, **props):
  return {'kind': kind, 'text': text, 'rect': {'x': x, 'y': y, 'width': w, 'height': h}, 'props': props}


scenes = [
  ('wrapped', False, 'en', 3, [element('unified', 'Shared native UI\nA verylongwordwithoutspaces crosses the next line.\n\nTail', 18, 12, 500, 210, size=28)]),
  ('emoji', False, 'en', 3, [element('label', 'Ready 😀\nWi-Fi 🇰🇷', 20, 15, 496, 210, size=38)]),
  ('scroll', False, 'en', 80, [element('unified', 'Scrolling a long shared framework label through the viewport', 30, 60, 460, 80, size=38, scroll=True)]),
  ('shimmer', False, 'en', 20, [element('unified', 'Slide to continue', 18, 30, 500, 180, size=42, shimmer=True, horizontal='right', vertical='middle')]),
  (
    'controls',
    False,
    'en',
    3,
    [
      element('button', 'Continue', 8, 8, 250, 90, size=32, style='primary'),
      element('button', 'Delete', 274, 8, 250, 90, size=32, style='danger'),
      element('toggle', '', 10, 140, 160, 80, value=True),
      element('toggle', '', 200, 140, 160, 80, value=False),
      element('toggle', '', 370, 140, 160, 80, value=True, enabled=False),
    ],
  ),
  (
    'korean',
    True,
    'ko',
    3,
    [
      element('unified', '기본 인터페이스\n네트워크 연결을 확인하고 계속 진행하세요.\n원본 글꼴과 줄바꿈을 유지합니다.', 60, 50, 2040, 700, size=90),
      element('button', '계속', 1400, 820, 650, 180, style='primary'),
    ],
  ),
]
results = []
for name, big, language, frames, elements in scenes:
  scene = {'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0}, 'language': language, 'frames': frames, 'elements': elements}
  path = args.output / f'{name}.json'
  path.write_text(json.dumps(scene, ensure_ascii=False))
  images = []
  states = []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{name}.png'
    command = (
      [sys.executable, str(root / 'rust/tools/ui_framework_source_render.py'), str(path), str(output)]
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
(args.output / 'render-results.json').write_text(json.dumps(results, indent=2))
assert all(item['different_pixels'] == 0 and item['state_equal'] for item in results), results
print('PASS: six actual-source/native shared widget scenes and state snapshots')
