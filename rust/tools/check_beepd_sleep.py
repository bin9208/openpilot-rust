#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
# Run: python rust/tools/check_beepd_sleep.py --binary EXAMPLE --output DIRECTORY
"""Scalar CPython timeout conversion; valid long durations are never actually slept."""
import argparse
import ctypes
import json
import math
from pathlib import Path
import struct
import subprocess
import time


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  limit = 2**63 / 1e9
  values = [0.0, -0.0, 1e-320, 1e-12, 0.49e-9, 0.5e-9, 0.500001e-9, 1.0001e-9, 1.5e-9, 1.9999e-9,
            0.01, 0.02, 0.03, 0.04, 0.05, 0.1, math.nextafter(0.05, math.inf), 0.0499999999996362,
            -1e-320, -0.1, math.nan, math.inf, -math.inf, math.nextafter(limit, -math.inf), limit,
            math.nextafter(limit, math.inf), 1e20]
  convert = ctypes.pythonapi._PyTime_FromSecondsObject
  convert.argtypes = [ctypes.POINTER(ctypes.c_int64), ctypes.py_object, ctypes.c_int]
  convert.restype = ctypes.c_int
  bits = [struct.unpack('Q', struct.pack('d', value))[0] for value in values]
  expected = []
  for value in values:
    try:
      duration = ctypes.c_int64()
      assert convert(ctypes.byref(duration), value, 3) == 0  # _PyTime_ROUND_TIMEOUT = _PyTime_ROUND_UP
      if duration.value < 0:
        raise ValueError('sleep length must be non-negative')
      expected.append({'nanoseconds': str(duration.value)})
    except (ValueError, OverflowError) as error:
      # Only already-rejected durations reach the real sleep API; none can block.
      try:
        time.sleep(value)
      except (ValueError, OverflowError) as public_error:
        assert type(public_error) is type(error), (value, public_error, error)
      else:
        raise AssertionError(('invalid sleep accepted', value))
      expected.append({'error': type(error).__name__})
  config = {'mode': 'sleep', 'root': '/unused', 'trace': '/unused', 'sleep_bits': bits}
  result = subprocess.run([str(args.binary.resolve())], input=json.dumps(config) + '\n', capture_output=True, text=True, check=True)
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  rows = [{'seconds_bits': value, 'source': left, 'native': right} for value, left, right in zip(bits, expected, actual, strict=True)]
  failures = [row for row in rows if ('error' in row['source']) != ('error' in row['native']) or
              ('nanoseconds' in row['source'] and row['source'] != row['native'])]
  (args.output / 'native.stdout').write_text(result.stdout)
  (args.output / 'summary.json').write_text(json.dumps({'passed': not failures, 'cases': len(rows), 'failures': failures, 'results': rows}, indent=2) + '\n')
  print(json.dumps({'passed': not failures, 'cases': len(rows), 'failures': len(failures)}))
  assert not failures, failures


if __name__ == '__main__':
  main()
