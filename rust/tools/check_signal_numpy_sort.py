"""Actual NumPy argsort index order, including portable target tie/NaN behavior."""
import argparse
import hashlib
import json
from pathlib import Path
import numpy as np


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rng = np.random.default_rng(76594)
  cases, expected = [], []
  for index in range(1920):
    length = (0, 1, 2, 15, 16, 17, 32, 65, 127, 256, 1024, 3549)[index % 12]
    values = rng.integers(-5, 6, length).astype(np.float32)
    mode = index // 12 % 8
    if mode == 0:
      values[:] = 0.
    elif mode == 1:
      values = np.arange(length, dtype=np.float32)
    elif mode == 2:
      values = -np.arange(length, dtype=np.float32)
    elif mode == 3:
      values[::3] = np.nan
      values[1::5] = np.inf
      values[2::7] = -np.inf
    elif mode == 4:
      values[:] = -0.
      values[::2] = 0.
    elif mode == 5:
      values[:] = np.nan
    elif mode == 6:
      values = np.abs(np.arange(length, dtype=np.float32) - length // 2)
    cases.append([int(v) for v in values.view(np.uint32)])
    expected.append(np.argsort(values).tolist())
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  (args.output / 'input.txt').write_text(''.join(' '.join(map(str, case)) + '\n' for case in cases))
  (args.output / 'expected.txt').write_text(''.join(' '.join(map(str, case)) + '\n' for case in expected))
  (args.output / 'scope.json').write_text(json.dumps(dict(numpy=np.__version__, revision=np.version.git_revision, architecture=__import__('platform').machine(), cases=len(cases), keys=sum(map(len,cases)), native_compared=False)))
  print(json.dumps(dict(cases=len(cases), keys=sum(map(len,cases)), scope='actual_numpy_argsort_portable_target_or_simd_host')))


if __name__ == '__main__':
  main()
