#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pycryptodome==3.23.0"]
# ///
# How to run: oracle Python rust/tools/check_card_toyota_secoc.py --binary PATH --evidence DIR
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import subprocess
from can_source import ROOT, load


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  load()
  import Crypto
  from opendbc.car.secoc import add_mac, build_sync_mac
  assert Crypto.__version__ == '3.23.0'
  rng = random.Random(177126)
  cases = []
  for size, trip, reset, message in itertools.product((16, 24, 32), (0, 1, 65535), (0, 1, 2, 3, 15, 1048575), (0, 1, 2, 3, 255, 256, 257, 65535)):
    cases.append({'key': list(rng.randbytes(size)), 'trip': trip, 'reset': reset, 'message': message,
                  'frame': {'address': (0x191, 0x131, 0xffff)[size // 8 - 2], 'data': list(rng.randbytes(8)), 'bus': 0}})
  for length in (0, 1, 2, 3, 4, 7, 8, 12):
    cases.append({'key': list(b'00' * 16), 'trip': 123, 'reset': 987, 'message': 255,
                  'frame': {'address': 0x191, 'data': list(rng.randbytes(length)), 'bus': 2}})
  expected = []
  for case in cases:
    key = bytes(case['key'])
    frame = case['frame']
    address, data, bus = add_mac(key, case['trip'], case['reset'], case['message'], (frame['address'], bytes(frame['data']), frame['bus']))
    expected.append({'sync': build_sync_mac(key, case['trip'], case['reset']), 'frame': {'address': address, 'data': list(data), 'bus': bus}})
  output = args.evidence.resolve() / 'native.json'
  (args.evidence / 'input.json').write_text(json.dumps(cases) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  child = subprocess.run([str(args.binary.resolve()), str(output)], input=json.dumps(cases), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  assert actual == expected
  report = {'status': 'pass', 'cases': len(cases), 'aes_key_bits': [128, 192, 256], 'source_sha256': hashlib.sha256((ROOT / 'opendbc_repo/opendbc/car/secoc.py').read_bytes()).hexdigest(),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'pycryptodome': Crypto.__version__}
  (args.evidence / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
