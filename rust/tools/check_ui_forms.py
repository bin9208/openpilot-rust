"""Compare real source/native forms, input histories and rendered pixels."""

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
parser.add_argument('--only')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1', PYTHONPATH=str(root), PARAMS_ROOT=str(args.output / 'params'))


def event(frame, x, y, pressed=False, released=False, down=False):
  return {'frame': frame, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': pressed, 'released': released, 'down': down, 'time': frame / 20}]}


def click(frame, x, y):
  return [event(frame, x, y, pressed=True, down=True), event(frame + 1, x, y, released=True)]


scenes = [
  ('debug-input', 'input', True, 3, 'Debug input', {'show_touches': True}, []),
  ('mici-idle', 'mici', False, 4, '', {}, []),
  ('mici-active', 'mici', False, 7, '', {}, [event(1, 52, 113, pressed=True, down=True), event(2, 160, 166, down=True), event(3, 270, 166, down=True)]),
  ('keyboard-initial', 'keyboard', True, 3, 'long-existing-password-abcdefghijklmnopqrstuvwxyz-0123456789', {'password': True, 'toggle': True}, []),
  (
    'mici-type',
    'mici',
    False,
    100,
    '',
    {'auto_return': './'},
    click(2, 52, 113)
    + click(10, 106, 113)
    + click(20, 52, 216)
    + click(30, 159, 113)
    + click(40, 484, 216)
    + click(50, 430, 163)
    + [{'frame': 60, 'operation': 'space'}, {'frame': 70, 'operation': 'backspace'}],
  ),
  (
    'html',
    'html',
    True,
    3,
    '<h1>Shared forms</h1><p>One paragraph with <b>bold words</b> and tail.</p><ul><li>First</li><ul><li>Nested text</li></ul></ul><br><h3>Finish</h3>',
    {},
    [],
  ),
  ('html-center', 'html', True, 3, '<h2>Centered content</h2><p>Ready for the next step.</p>', {'center': True}, []),
  (
    'input',
    'input',
    True,
    100,
    '',
    {},
    [{'frame': i, 'operation': 'character', 'text': c} for i, c in enumerate('Openpilot text')]
    + [{'frame': 20, 'key': 268}, {'frame': 21, 'key': 261}, {'frame': 23, 'key': 269}, {'frame': 24, 'key': 259}]
    + [{'frame': i, 'down': [259]} for i in range(25, 85)],
  ),
  (
    'password',
    'input',
    True,
    65,
    '',
    {'password': True, 'max': 8},
    [{'frame': i, 'operation': 'character', 'text': c} for i, c in enumerate('Secret12345')] + [{'frame': 20, 'operation': 'cursor', 'value': 3}],
  ),
  (
    'keyboard',
    'keyboard',
    True,
    30,
    '',
    {'password': True, 'toggle': True},
    [{'frame': i * 2, 'operation': 'key', 'text': c} for i, c in enumerate(['SHIFT_OFF', 'SHIFT_ON', 'A', 'CAPS', 'b', '123', '#+=', '€', 'ABC', 'x'])],
  ),
  ('confirm', 'confirm', True, 3, 'Do you want to continue?', {}, []),
  (
    'confirm-rich',
    'confirm',
    True,
    3,
    '<h2>Review changes</h2><p>This dialog preserves the original layout.</p><ul><li>First item</li><li>Second item</li></ul>',
    {'rich': True},
    [],
  ),
  ('confirm-key', 'confirm', True, 3, 'Confirm with keyboard', {'cancel': ''}, [{'frame': 1, 'key': 257}]),
  ('options', 'options', True, 4, 'Choose an option', {}, [{'frame': 1, 'operation': 'selection', 'text': 'Third'}]),
  ('list-toggle', 'list', True, 4, 'Shared setting', {'description': 'A detailed setting explanation.', 'description_visible': True}, []),
  ('list-button', 'list', True, 4, 'Shared setting', {'action': 'button'}, []),
  ('list-multiple', 'list', True, 4, 'Shared setting', {'action': 'multiple'}, []),
  (
    'slider',
    'slider',
    False,
    45,
    'Slide to confirm',
    {'green': True},
    [
      event(1, 485, 175, pressed=True, down=True),
      event(2, 360, 175, down=True),
      event(3, 140, 175, down=True),
      event(4, 140, 175, down=True),
      event(5, 140, 175, released=True),
    ],
  ),
]


def equal(a, b, where='root'):
  if isinstance(a, dict) and isinstance(b, dict):
    assert a.keys() == b.keys(), (where, a.keys(), b.keys())
    for key in a:
      equal(a[key], b[key], f'{where}.{key}')
  elif isinstance(a, list) and isinstance(b, list):
    assert len(a) == len(b), (where, len(a), len(b))
    for i, (x, y) in enumerate(zip(a, b, strict=True)):
      equal(x, y, f'{where}[{i}]')
  elif isinstance(a, (int, float)) and isinstance(b, (int, float)):
    assert abs(a - b) <= 2e-5, (where, a, b)
  else:
    assert a == b, (where, a, b)


results = []
for name, kind, big, frames, text, props, actions in scenes:
  if args.only and name not in args.only.split(','):
    continue
  rect = {'x': 0, 'y': 0, 'width': 2160 if big else 536, 'height': 1080 if big else 240}
  if kind == 'input':
    rect = {'x': 100, 'y': 150, 'width': 800, 'height': 180}
  elif kind == 'list':
    rect = {'x': 100, 'y': 100, 'width': 1800, 'height': 170}
  elif kind == 'slider':
    rect = {'x': 0, 'y': 120, 'width': 536, 'height': 115}
  scene = {
    'config': {'big': big, 'large_viewport': big, 'pc': True, 'scale': 1.0},
    'language': 'en',
    'kind': kind,
    'rect': rect,
    'text': text,
    'frames': frames,
    'props': props,
    'actions': actions,
  }
  path = args.output / f'{name}.json'
  path.write_text(json.dumps(scene))
  captures, states = [], []
  for lane in ['source', 'native']:
    output = args.output / f'{lane}-{name}.png'
    command = (
      [sys.executable, str(root / 'rust/tools/ui_forms_source.py'), str(path), str(output)]
      if lane == 'source'
      else [str(args.binary), str(root), str(path), str(output)]
    )
    with (args.output / f'{lane}-{name}.log').open('w') as log:
      subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    captures.append(np.asarray(Image.open(output)).astype(int))
    states.append(json.loads(output.with_suffix('.json').read_text()))
  delta = abs(captures[0] - captures[1])
  result = {
    'scene': name,
    'frames': frames,
    'different_pixels': int(np.any(delta != 0, axis=-1).sum()),
    'max_channel_difference': int(delta.max()),
    'state_equal': True,
  }
  try:
    equal(*states)
  except AssertionError as error:
    result['state_equal'] = False
    result['state_error'] = str(error)
  results.append(result)
  print(json.dumps(result), flush=True)
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(item['different_pixels'] == 0 and item['state_equal'] for item in results), results
print(f'PASS: {len(results)} actual-source/native form scenes; {sum(item["frames"] for item in results)} input/render frames')
