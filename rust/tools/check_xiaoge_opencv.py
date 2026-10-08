#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3"]
# ///
# Run retained reference Python with --binary opencv_trace --fixtures fixtures.json --output evidence.
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from enum import StrEnum
from typing import TypedDict, assert_never
import numpy as np

class Kind(StrEnum):
  IMAGE = 'image'
  BOUNDS = 'bounds'
  DNN = 'dnn'

class FloatComparison(TypedDict):
  elements: int
  bitwise_equal: bool
  changed_elements: int
  max_absolute_error: float
  max_relative_error: float


def floats(expected: Path, actual: Path) -> FloatComparison:
  left = np.fromfile(expected, dtype='<f4')
  right = np.fromfile(actual, dtype='<f4')
  assert left.shape == right.shape, (left.shape, right.shape)
  assert np.all(np.isfinite(left)) and np.all(np.isfinite(right))
  difference = np.abs(left.astype(np.float64) - right.astype(np.float64))
  denominator = np.maximum(np.abs(left.astype(np.float64)), np.finfo(np.float32).tiny)
  return {'elements': int(left.size), 'bitwise_equal': expected.read_bytes() == actual.read_bytes(),
    'changed_elements': int(np.count_nonzero(left.view(np.uint32) != right.view(np.uint32))),
    'max_absolute_error': float(np.max(difference)), 'max_relative_error': float(np.max(difference / denominator))}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixtures', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir()
  corpus = json.loads(args.fixtures.read_text())
  assert corpus['wheel_defaults_preserved'] is True
  for name, digest in corpus['sha256'].items():
    assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == digest, name
  rows = []
  for case in corpus['cases']:
    name = case['name']
    request = args.output / (name + '-input.json')
    response = args.output / (name + '-output.json')
    request.write_text(json.dumps(case['request']) + '\n')
    command = [*args.runner, str(args.binary), str(request), str(response)]
    result = subprocess.run(command, text=True, capture_output=True, check=False)
    (args.output / (name + '.stdout')).write_text(result.stdout)
    (args.output / (name + '.stderr')).write_text(result.stderr)
    row = {'name': name, 'argv': command, 'returncode': result.returncode, 'status': 'fail'}
    if result.returncode == 0:
      actual = json.loads(response.read_text())
      expected = case['expected']
      maps = response.with_suffix('.maps.txt').read_text()
      assert 'libpython' not in maps and 'cv2.abi3.so' not in maps
      kind = Kind(expected['kind'])
      match kind:
        case Kind.IMAGE:
          left = Path(expected['file']).read_bytes()
          right = Path(actual['file']).read_bytes()
          identical = left == right
          extent = [actual['height'], actual['width']] == expected['shape'][:2]
          row.update({'bytes': len(left), 'bitwise_equal': identical, 'dimensions_equal': extent,
            'changed_bytes': sum(a != b for a, b in zip(left, right, strict=True)),
            'status': 'pass' if identical and extent else 'fail'})
        case Kind.BOUNDS:
          row['status'] = 'pass' if actual == expected else 'fail'
        case Kind.DNN:
          assert len(actual['outputs']) == len(expected['outputs'])
          outputs = []
          for source, native in zip(expected['outputs'], actual['outputs'], strict=True):
            metric = {'floats': floats(Path(source['file']), Path(native['file'])), 'shape_equal': source['shape'] == native['shape']}
            outputs.append(metric)
          names_equal = expected['available_names'] == actual['available_names']
          row.update({'outputs': outputs, 'names_equal': names_equal,
            'status': 'pass' if names_equal and all(value['floats']['bitwise_equal'] and value['shape_equal'] for value in outputs) else 'fail'})
        case unknown:
          assert_never(unknown)
    rows.append(row)
  receipt = {'status': 'pass' if all(row['status'] == 'pass' for row in rows) else 'fail', 'cases': rows,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'fixture_sha256': hashlib.sha256(args.fixtures.read_bytes()).hexdigest(),
    'environment': {key: os.environ.get(key) for key in ('LD_LIBRARY_PATH', 'ASAN_OPTIONS', 'UBSAN_OPTIONS')},
    'scope': 'exact primitive pixels and actual original preprocessing + pinned real-model output bytes/shapes/names; no device or CPU claim'}
  (args.output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({'status': receipt['status'], 'cases': len(rows), 'failures': [row['name'] for row in rows if row['status'] != 'pass']}))
  raise SystemExit(0 if receipt['status'] == 'pass' else 1)


if __name__ == '__main__':
  main()
