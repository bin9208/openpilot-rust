"""Compare neural outputs, including Float32 bit patterns and signed zero."""

import argparse
import json
from pathlib import Path

import numpy as np


def compare_models(expected, actual):
  assert expected['exp'] == actual['exp']
  assert expected['flux'].keys() == actual['flux'].keys()
  for name, model in expected['flux'].items():
    assert model['friction'] == actual['flux'][name]['friction'], name
    left = np.asarray(model['values'], dtype=np.float32).view(np.uint32)
    right = np.asarray(actual['flux'][name]['values'], dtype=np.float32).view(np.uint32)
    assert np.array_equal(left, right), (name, np.flatnonzero(left != right)[:8].tolist())
  assert expected['nano'].keys() == actual['nano'].keys()
  max_error = 0.0
  nano_bits = True
  for name, values in expected['nano'].items():
    left = np.asarray(values, dtype=np.float64)
    right = np.asarray(actual['nano'][name], dtype=np.float64)
    assert np.allclose(left, right, atol=1e-9, rtol=1e-9), name
    max_error = max(max_error, float(np.max(np.abs(left - right))))
    nano_bits &= bool(np.array_equal(left.view(np.uint64), right.view(np.uint64)))
  return {
    'pass': True,
    'flux_models': len(expected['flux']),
    'nano_models': len(expected['nano']),
    'inputs_per_model': len(next(iter(expected['flux'].values()))['values']),
    'exp_samples': len(expected['exp']),
    'flux_bit_identical': True,
    'nano_bit_identical': nano_bits,
    'nano_max_absolute_error': max_error,
  }


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  result = compare_models(json.loads((args.evidence / 'source.json').read_text()), json.loads((args.evidence / 'native.json').read_text()))
  (args.evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
