"""Compare real SystemTime conversion with Python UTC datetime and PyJWT NumericDate."""

import argparse
import calendar
from datetime import datetime, timedelta, UTC
import hashlib
import json
from pathlib import Path
import subprocess


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  cases = [
    (0, 0),
    (0, 999_999_999),
    (-1, 999_999_999),
    (-1, 0),
    (-2, 999_999_999),
    (-62_135_596_800, 0),
    (-62_135_596_801, 999_999_999),
    (253_402_297_199, 999_999_999),
    (253_402_297_200, 0),
    (253_402_300_799, 999_999_999),
    (253_402_300_800, 0),
    (2**63 - 1, 0),
    (-(2**63), 0),
  ]
  expected = []
  for seconds, nanos in cases:
    value, expiration = None, None
    try:
      now = datetime.fromtimestamp(seconds, UTC).replace(microsecond=nanos // 1000)
      value = calendar.timegm(now.utctimetuple())
      expiry = now.replace(tzinfo=None) + timedelta(hours=1)
      expiration = calendar.timegm(expiry.utctimetuple())
    except (ValueError, OverflowError, OSError):
      pass
    expected.append({'seconds': value, 'expiration': expiration})
  command = [*args.runner, str(args.binary.resolve())]
  run = subprocess.run(command, input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, check=True)
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  report = {
    'argv': command,
    'cases': cases,
    'source': expected,
    'native': actual,
    'exit': run.returncode,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'equal': expected == actual,
  }
  args.output.write_text(json.dumps(report, indent=2))
  assert expected == actual, report
  print('13 real SystemTime/Python UTC floor, calendar and one-hour expiry boundary cases PASS')


if __name__ == '__main__':
  main()
