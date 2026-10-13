from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import numpy as np
from PIL import Image


def compare_values(expected, actual, name: str, differences: list[dict], errors: list[dict]) -> None:
  if isinstance(expected, dict):
    if set(expected) != set(actual):
      errors.append({'name': name, 'expected_keys': list(expected), 'actual_keys': list(actual)})
      return
    for key, value in expected.items():
      compare_values(value, actual[key], f'{name}.{key}', differences, errors)
  elif isinstance(expected, list):
    if not isinstance(actual, list) or len(expected) != len(actual):
      errors.append({'name': name, 'expected_length': len(expected), 'actual': actual})
      return
    for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
      compare_values(left, right, f'{name}[{index}]', differences, errors)
  elif isinstance(expected, float):
    if not isinstance(actual, (int, float)) or not math.isclose(expected, actual, abs_tol=1e-10, rel_tol=1e-12):
      errors.append({'name': name, 'expected': expected, 'actual': actual})
    elif expected != actual:
      differences.append({'name': name, 'absolute': abs(expected - actual)})
  elif expected != actual:
    errors.append({'name': name, 'expected': expected, 'actual': actual})


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('source', type=Path)
  parser.add_argument('native', type=Path)
  parser.add_argument('result', type=Path)
  args = parser.parse_args()
  source = json.loads((args.source / 'source-result.json').read_text())
  native = json.loads((args.native / 'native-result.json').read_text())
  errors, differences, captures = [], [], []
  compare_values(source['helpers'], native['helpers'], 'helpers', differences, errors)
  expected = {(item['name'], item['warm']): item for item in source['captures']}
  actual = {(item['name'], item['warm']): item for item in native['captures']}
  assert expected.keys() == actual.keys(), 'capture scenarios differ'
  for key, left in expected.items():
    right = actual[key]
    original = np.array(Image.open(left['path']).convert('RGBA'))
    translated = np.array(Image.open(right['path']).convert('RGBA'))
    assert original.shape == translated.shape, key
    changed = np.any(original != translated, axis=2)
    count = int(changed.sum())
    captures.append({'name': key[0], 'warm': key[1], 'changed_pixels': count, 'source': left['path'], 'native': right['path']})
    if count:
      ys, xs = np.where(changed)
      errors.append({'name': key, 'changed_pixels': count, 'bounds': [int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())]})
    assert np.array_equal(translated[5:15, translated.shape[1] - 20 : translated.shape[1] - 10], [255, 0, 255, 255] * np.ones((10, 10, 1), dtype=np.uint8)), (
      'scissor leaked'
    )
  expected_buffers = {item['name']: item for item in source['buffers']}
  stages = ['render_world.camera_projected_overlay.strips', 'render_world.camera_projected_overlay.radar', 'render_world.camera_projected_overlay.vehicles'] * 2
  for item in native['buffers']:
    original = expected_buffers[item['name']]
    assert (item['points'], item['visible']) == (original['points'], original['visible']), item
    assert item['native'] == (item['points'] if source['strip_available'] else 0), item
    assert item['strip_available'] == source['strip_available'], item
    samples = [(name, elapsed) for name, elapsed in item['profile'] if name.startswith('render_world.camera_projected_overlay.')]
    assert [name for name, _ in samples] == stages, item
    assert all(math.isfinite(elapsed) and elapsed >= 0 for _, elapsed in samples), item
  assert source['closed_points'] == source['closed_visible'] == 0
  assert all(item['points'] == item['visible'] == item['native'] == 0 for item in native['closes'])
  result = {
    'helper_cases': len(source['helpers']),
    'projected_points': sum(len(item['points']) for item in source['helpers']),
    'frames': len(captures),
    'captures': captures,
    'errors': errors,
    'floating_differences': len(differences),
    'max_absolute_difference': max((item['absolute'] for item in differences), default=0.0),
    'strip_available': source['strip_available'],
    'buffer_cases': len(expected_buffers),
    'closed_buffers': native['closes'],
    'helper_tolerance': {'absolute': 1e-10, 'relative': 1e-12},
    'pixel_tolerance': 0,
  }
  args.result.write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({key: value for key, value in result.items() if key != 'captures'}))
  assert not errors, f'{len(errors)} source mismatches'


if __name__ == '__main__':
  main()
