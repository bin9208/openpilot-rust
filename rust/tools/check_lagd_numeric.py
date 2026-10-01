#!/usr/bin/env python3
"""Source/numeric parity including masking, candidate choices and pre-fixed tolerances."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np
from lagd_reference import ROOT, compare, numeric, source


def cases():
  yield 'padding', {'kind': 'padding', 'values': list(range(1, 150)) + [499, 501, 999, 1200, 1220, 1225, 2049, 10001]}, 'lag_and_block_statistics'
  for values, index in [([3.0, 2.0, 1.0], 0), ([1.0, 2.0, 3.0], 2), ([0.0, 2.0, 0.0], 1), ([0.1, 0.8, 0.5], 1), ([1.0, 1.0, 1.0], 1)]:
    yield f'peak-{index}-{values}', {'kind': 'peak', 'values': values, 'index': index}, 'lag_and_block_statistics'
  rng = np.random.default_rng(139)
  for count in [40, 200, 500, 1200]:
    times = np.arange(count) * 0.05
    base = 0.8 * np.sin(times * 0.73) + 0.2 * np.sin(times * 2.31) + 0.1 * np.cos(times * 4.3)
    actual = 0.8 * np.sin((times - 0.31) * 0.73) + 0.2 * np.sin((times - 0.31) * 2.31) + 0.1 * np.cos((times - 0.31) * 4.3)
    for mask_name, mask in [('all', np.ones(count, dtype=bool)), ('holes', rng.random(count) > 0.2), ('empty', np.zeros(count, dtype=bool))]:
      yield f'smooth-{count}-{mask_name}', {'kind': 'smooth', 'values': base.tolist(), 'mask': mask.tolist(), 'k': 5, 'sigma': 1.0}, 'pose_and_smoothing'
      row = {'expected': base.tolist(), 'actual': actual.tolist(), 'mask': mask.tolist()}
      n = source()['fft_next_good_size'](count + 20)
      yield f'ncc-{count}-{mask_name}', {'kind': 'correlate', 'n': n, **row}, 'correlation'
      yield f'delay-{count}-{mask_name}', {'kind': 'delay', 'dt': 0.05, 'min': 0.15, 'max': 1.0, **row}, 'lag_and_block_statistics'
  for kind in ['zero', 'constant', 'impulse', 'alternating']:
    values = {'zero': np.zeros(1200), 'constant': np.full(1200, 0.1), 'impulse': np.eye(1, 1200, 500).ravel(), 'alternating': np.tile([0.0, 1.0], 600)}[kind]
    row = {'expected': values.tolist(), 'actual': np.roll(values, 6).tolist(), 'mask': [True] * 1200}
    yield 'degenerate-ncc-' + kind, dict(kind='correlate', n=1225, **row), 'correlation'
    yield 'degenerate-delay-' + kind, dict(kind='delay', dt=0.05, min=0.15, max=1.0, **row), 'lag_and_block_statistics'
  constant_rng = np.random.default_rng(13901)
  for count in [200, 1200]:
    for amplitude in [0.1, 1.01]:
      for mask_name, mask in [
        ('holes', constant_rng.random(count) > 0.2),
        ('periodic', np.arange(count) % 5 != 0),
        ('sparse', np.arange(count) % 20 == 0),
      ]:
        row = {'expected': [amplitude] * count, 'actual': [amplitude] * count, 'mask': mask.tolist()}
        name = f'masked-constant-{count}-{amplitude}-{mask_name}'
        yield name + '-ncc', dict(kind='correlate', n=source()['fft_next_good_size'](count + 20), **row), 'correlation'
        yield name + '-delay', dict(kind='delay', dt=0.05, min=0.15, max=1.0, **row), 'lag_and_block_statistics'
  for valid in [-1, 0, 1, 4, 5]:
    yield (
      f'blocks-{valid}',
      {'kind': 'blocks', 'count': 5, 'size': 3, 'initial': 0.4, 'valid': valid, 'updates': rng.uniform(0.15, 1.0, 40).tolist()},
      'lag_and_block_statistics',
    )
  for index in range(16):
    pose = {
      name: {'xyz': rng.normal(size=3).tolist(), 'std': rng.uniform(0.01, 1.0, 3).tolist()}
      for name in ['orientation', 'velocity', 'acceleration', 'angular_velocity']
    }
    yield f'pose-{index}', {'kind': 'pose', 'pose': pose, 'rpy': rng.uniform(-2.0, 2.0, 3).tolist(), 'valid': index % 2 == 0}, 'pose_and_smoothing'


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  provenance = json.loads((ROOT / 'rust/crates/pocketfft/provenance.json').read_text())
  assert np.__version__ == provenance['numpy_version']
  assert np.version.git_revision == provenance['numpy_commit']
  for name, digest in provenance['files'].items():
    assert hashlib.sha256((ROOT / 'rust/crates/pocketfft/native/vendor' / name).read_bytes()).hexdigest() == digest
  assert (
    hashlib.sha256((ROOT / 'rust/crates/lagd/tests/tolerances.json').read_bytes()).hexdigest()
    == '5f8d76478bf89581b2c7e25e7e122cd61fb3ddcb3cf63f59ece3b2c9c22e766b'
  )
  all_cases = list(cases())
  result = subprocess.run(
    [args.binary], input=''.join(json.dumps(row) + '\n' for _, row, _ in all_cases), capture_output=True, text=True, timeout=30, check=True
  )
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  scope = source()
  budgets = json.loads((ROOT / 'rust/crates/lagd/tests/tolerances.json').read_text())
  records = []
  failures = []
  for (name, row, budget), value in zip(all_cases, actual, strict=True):
    expected = numeric(scope, row)
    try:
      compare(expected, value, budgets[budget], name)
    except AssertionError as error:
      failures.append(str(error))
    records.append({'case': name, 'input': row, 'source': expected, 'native': value})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps({'records': records, 'failures': failures}, indent=2) + '\n')
  assert not failures, '\n'.join(failures[:12])
  print(f'PASS: {len(records)} original-source numeric/candidate scenarios with unchanged preimplementation tolerances')


if __name__ == '__main__':
  main()
