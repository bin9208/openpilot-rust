"""Differential UTF-7 replacement and surrogate tests against the actual Python codec."""

import argparse
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
  cases = [b'', b'plain ASCII', b'+', b'+-', b'+?', b'\xff', b'prefix' + bytes(range(128, 256)) + b'suffix']
  for middle in [b'A', b'AEI', b'AEJ', b'2AA', b'3AA', b'2ADcAA', b'2ADYAA']:
    for end in [b'', b'-', b'?', b'\xff', b'+-']:
      cases.append(b'prefix+' + middle + end + b'suffix')
      cases.append(b'+' + middle + end)
  for points in [[0], [0x80], [0xFFFF], [0x1F600], [0xD800], [0xDC00], [0xD800, 65], [65, 0xDC00], [0xD800, 0xD800], [0xD800, 0xDC00]]:
    encoded = ''.join(chr(point) for point in points).encode('utf-7')
    for tail in range(min(5, len(encoded))):
      cases.append(encoded[: len(encoded) - tail])
  cases = list(dict.fromkeys(cases))
  expected = [[ord(character) for character in data.decode('utf-7', errors='replace')] for data in cases]
  command = [*args.runner, str(args.binary.resolve())]
  run = subprocess.run(command, input=''.join(json.dumps(list(case)) + '\n' for case in cases), text=True, capture_output=True, check=True)
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  assert len(actual) == len(expected)
  failures = [{'hex': data.hex(), 'source': left, 'native': right} for data, left, right in zip(cases, expected, actual, strict=True) if left != right]
  report = {
    'argv': command,
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'exit': run.returncode,
    'cases': [{'hex': data.hex(), 'source': left, 'native': right} for data, left, right in zip(cases, expected, actual, strict=True)],
    'failures': failures,
  }
  args.output.write_text(json.dumps(report, indent=2))
  assert not failures, failures
  print(f'{len(cases)} UTF-7 valid/malformed/padding/truncation/direct-byte/surrogate cases PASS')


if __name__ == '__main__':
  main()
