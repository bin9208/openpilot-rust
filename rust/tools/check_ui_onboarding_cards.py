"""Actual compact onboarding cards, navigation and confirmations in both languages."""

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
parser.add_argument('--display', default=':127')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
cases = []
for kind, count in [('terms', 6), ('attention', 6), ('pre-dm', 5), ('bad-face', 3), ('record-front', 4)]:
  steps = []
  for page in range(count):
    steps.append({'frame': 20 + page * 3, 'scroll_item': page})
  cases.append((kind, {'kind': kind, 'frames': 20 + count * 3, 'steps': steps}))


def event(frame, x, y, kind):
  return {
    'frame': frame,
    'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': kind == 'press', 'released': kind == 'release', 'down': kind != 'release', 'time': frame / 20}],
  }


for name, kind, item, expected, value in [
  ('terms-accept', 'terms', 4, 'accept', None),
  ('terms-decline', 'terms', 5, 'decline', None),
  ('record-yes', 'record-front', 2, 'next', True),
  ('record-no', 'record-front', 3, 'next', False),
]:
  steps = [
    {'frame': 20, 'scroll_item': item},
    event(22, 350 if name in ['terms-decline', 'record-no'] else 268, 120, 'press'),
    event(23, 350 if name in ['terms-decline', 'record-no'] else 268, 120, 'release'),
    event(48, 445, 120, 'press'),
    event(49, 100, 120, 'move'),
    event(50, 100, 120, 'release'),
  ]
  cases.append(
    (
      name,
      {'kind': kind, 'frames': 90, 'steps': steps, 'params': {'RecordFront': '1' if value is False else '0'}, 'expected': expected, 'record_expected': value},
    )
  )
for name, steps in [
  ('cancel', [event(48, 268, 10, 'press'), event(49, 268, 220, 'move'), event(50, 268, 220, 'release')]),
  ('incomplete', [event(48, 445, 120, 'press'), event(49, 330, 120, 'move'), event(50, 330, 120, 'release')]),
]:
  cases.append(
    (
      f'terms-{name}',
      {
        'kind': 'terms',
        'frames': 90,
        'steps': [{'frame': 20, 'scroll_item': 4}, event(22, 268, 120, 'press'), event(23, 268, 120, 'release'), *steps],
        'expected': 'none',
      },
    )
  )
results = []
for language in ['en', 'ko']:
  for name, detail in cases:
    case = f'{name}-{language}'
    scene = {'config': {'big': False, 'large_viewport': False, 'pc': True, 'scale': 1.0}, 'language': language, 'prime': 0}
    scene.update(detail)
    path = args.output / f'{case}.json'
    path.write_text(json.dumps(scene))
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cards148-', dir='/dev/shm') as ns:
      env = dict(os.environ, DISPLAY=args.display, PYTHONPATH=str(root), OFFSCREEN='1', OPENPILOT_PREFIX=Path(ns).name.removeprefix('msgq_'))
      for lane in ['source', 'native']:
        output = args.output / f'{lane}-{case}.png'
        command = (
          [sys.executable, str(root / 'rust/tools/ui_application_qa/compact_cards_source.py'), str(path), str(output)]
          if lane == 'source'
          else [str(args.binary), str(root), str(path), str(output)]
        )
        with (args.output / f'{lane}-{case}.log').open('w') as log:
          subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    source = json.loads((args.output / f'source-{case}.json').read_text())
    native = json.loads((args.output / f'native-{case}.json').read_text())
    if 'expected' in scene:
      expected = ['confirm'] + ([] if scene['expected'] == 'none' else [scene['expected']])
      assert source[-1]['effects'] == expected, (case, source[-1])
      if scene.get('record_expected') is not None:
        assert source[-1]['record_front'] == scene['record_expected'], (case, source[-1])
    row = {'scene': case, 'state_equal': source == native, 'frames': []}
    for index in range(scene['frames']):
      a = np.asarray(Image.open(args.output / f'source-{case}-{index}.png'))
      b = np.asarray(Image.open(args.output / f'native-{case}-{index}.png'))
      row['frames'].append({'index': index, 'different_pixels': int(np.any(a != b, axis=-1).sum())})
      if any(step['frame'] + 1 == index and 'scroll_item' in step for step in scene['steps']):
        assert np.any(a[30:210, :, :3] != 0, axis=-1).sum() > 5000, ('empty card capture', case, index)
    results.append(row)
    print(json.dumps(row), flush=True)
    (args.output / 'results.json').write_text(json.dumps(results, indent=2))
assert all(r['state_equal'] and all(f['different_pixels'] == 0 for f in r['frames']) for r in results), results
print('PASS every original/native card frame and state')
