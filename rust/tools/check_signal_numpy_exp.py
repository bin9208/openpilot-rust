"""Pinned NumPy Float32 exp numerical outputs, not floating-status/Python warnings."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import numpy as np


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  assert np.__version__ == '2.5.3'
  rng = np.random.default_rng(337)
  values = np.concatenate([rng.integers(0, 2**32, 200000, dtype=np.uint32),
                           rng.uniform(-110, 95, 100000).astype(np.float32).view(np.uint32),
                           np.arange(-100, 90, .00025, dtype=np.float32).view(np.uint32)])
  with np.errstate(all='ignore'):
    expected = np.exp(values.view(np.float32)).view(np.uint32)
  text = ''.join(str(int(x)) + '\n' for x in values).encode()
  raw = subprocess.check_output([str(args.binary.resolve())], input=text)
  actual = np.fromstring(raw.decode(), sep=' ', dtype=np.uint32)
  assert len(actual) == len(expected)
  mismatches = np.flatnonzero(actual != expected)
  receipt = dict(numpy=np.__version__, cases=len(values), differences=len(mismatches),
                 input_sha256=hashlib.sha256(values.tobytes()).hexdigest(),
                 expected_sha256=hashlib.sha256(expected.tobytes()).hexdigest(),
                 actual_sha256=hashlib.sha256(actual.tobytes()).hexdigest(),
                 scope='numerical outputs only; excludes floating status and Python warnings')
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))
  assert not len(mismatches)


if __name__ == '__main__':
  main()
