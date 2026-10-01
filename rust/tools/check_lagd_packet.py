#!/usr/bin/env python3
"""LiveDelay status, clipping, partial/full-ring and threshold boundary comparison."""

import argparse
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import numpy as np
import capnp
from openpilot.cereal import log
from lagd_reference import ROOT, compare, source, normalized


def cases():
  for blocks in [0, 4, 5, 50, -1, -7]:
    yield f'blocks-{blocks}', {'values': [0.4] * 50, 'blocks': blocks, 'idx': 0, 'valid': True, 'debug': True}
  for value in [0.05, 1.5]:
    yield f'clamp-{value}', {'values': [value] * 50, 'blocks': 5, 'idx': 0, 'valid': False, 'debug': False}
  for delta in [-1e-12, 0.0, 1e-12]:
    values = [0.0, 0.2 + delta] * 3 + [0.4] * 44
    yield f'std-boundary-{delta}', {'values': values, 'blocks': 6, 'idx': 0, 'valid': True, 'debug': True}
  yield 'current-excluded-from-valid-std', {'values': [0.4] * 5 + [0.9] + [0.4] * 44, 'blocks': 5, 'idx': 1, 'valid': True, 'debug': True}
  yield 'full-ring-excludes-current', {'values': [0.0] + [0.4] * 49, 'blocks': 50, 'idx': 0, 'valid': True, 'debug': True}


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  rows = list(cases())
  result = subprocess.run([args.binary], input=''.join(json.dumps(row) + '\n' for _, row in rows), capture_output=True, text=True, check=True, timeout=10)
  native = [json.loads(line) for line in result.stdout.splitlines()]
  scope = source()
  budget = json.loads((ROOT / 'rust/crates/lagd/tests/tolerances.json').read_text())['cereal_float32']
  records = []
  for (name, row), actual in zip(rows, native, strict=True):
    estimator = scope['LateralLagEstimator'](SimpleNamespace(steerActuatorDelay=0.2), 0.05)
    estimator.reset(0.4, row['blocks'])
    estimator.block_avg.idx = row['idx']
    estimator.block_avg.values = np.array(row['values']).reshape(-1, 1)
    try:
      expected = normalized(estimator.get_msg(row['valid'], row['debug']).to_dict())
    except (OverflowError, ValueError, capnp.KjException):
      expected = {'error': True}
    if 'error' in expected:
      assert 'error' in actual, (name, actual)
    else:
      with log.Event.from_bytes(bytes(actual['bytes'])) as packet:
        actual = normalized(packet.to_dict())
      compare(expected, actual, budget, name)
    records.append({'case': name, 'source': expected, 'native': actual, 'pass_': True})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  print(f'PASS: {len(records)} source liveDelay status/clipping/progress/STD boundaries')


if __name__ == '__main__':
  main()
