#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
# Run: python rust/tools/check_beepd_clock.py --binary EXAMPLE --output DIRECTORY
"""Compare native monotonic conversion directly with CPython's time conversion."""
import argparse
import ctypes
import json
from pathlib import Path
import random
import struct
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  values = [0, 1, 999999999, 1000000000, 1000000001, 2**53 - 1, 2**53, 2**53 + 1,
            9000000000000000000, 2**63 - 1, -(2**63)]
  rng = random.Random(79)
  values.extend(rng.randrange(2**63) for _ in range(1000))
  convert = ctypes.pythonapi._PyTime_AsSecondsDouble
  convert.argtypes = [ctypes.c_int64]
  convert.restype = ctypes.c_double
  expected = [struct.unpack('Q', struct.pack('d', convert(value)))[0] for value in values]
  config = {'mode': 'clock', 'root': '/unused', 'trace': '/unused', 'timespecs': [divmod(value, 1000000000) for value in values]}
  result = subprocess.run([str(args.binary.resolve())], input=json.dumps(config) + '\n', capture_output=True, text=True, check=True)
  actual = [int(value) for value in result.stdout.splitlines()]
  (args.output / 'native.stdout').write_text(result.stdout)
  (args.output / 'inputs.json').write_text(json.dumps(config, indent=2) + '\n')
  differences = [{'nanoseconds': value, 'python_bits': left, 'native_bits': right}
                 for value, left, right in zip(values, expected, actual, strict=True) if left != right]
  report = {'passed': not differences, 'cases': len(values), 'differences': differences}
  (args.output / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': not differences, 'cases': len(values), 'differences': len(differences)}))
  assert not differences, differences[:10]


if __name__ == '__main__':
  main()
