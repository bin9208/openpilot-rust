#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import struct
import subprocess

from pigeon_source import trace, load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source = load()
  time_message = source['add_ubx_checksum'](b'\xb5\x62\x13\x40\x18\x00' + struct.pack('<BBBBHBBBBBxIHxxI', 0x10, 0, 0, 0x80, 2026, 9, 30, 12, 34, 56, 0, 30, 0))
  assist = source['add_ubx_checksum'](b'\xb5\x62\x13\x02\x01\x00\x33')
  results = []
  for mode, failure in [
    ('init', ''),
    ('init', 'nack'),
    ('initialize', 'timeout'),
    ('init', 'assist'),
    ('reset', ''),
    ('reset', 'timeout'),
    ('save', ''),
    ('save', 'nack'),
    ('save', 'timeout'),
  ]:
    name = mode + '-' + (failure or 'normal')
    request = {'mode': mode, 'failure': failure, 'time_message': list(time_message), 'token': 'fixture:+/,', 'assist': [list(assist)]}
    expected = trace(request)
    child = subprocess.run([args.trace.resolve()], input=json.dumps(request), text=True, capture_output=True)
    (args.evidence / f'pigeon-{name}.stderr').write_text(child.stderr or '(no stderr)\n')
    child.check_returncode()
    actual = json.loads(child.stdout)
    for kind, result in [('source', expected), ('native', actual)]:
      (args.evidence / f'pigeon-{name}-{kind}.json').write_text(json.dumps(result, indent=2) + '\n')
    assert actual['result'] == expected['result'], (name, actual['result'], expected['result'])
    for i, (a, b) in enumerate(zip(actual['trace'], expected['trace'], strict=True)):
      assert a == b, (name, i, a, b)
    results.append({'scenario': name, 'pass_': True, 'boundary_calls': len(actual['trace'])})
  (args.evidence / 'pigeon-results.json').write_text(json.dumps(results, indent=2) + '\n')
  print(f'PASS {len(results)} actual-source receiver policy scenarios')


if __name__ == '__main__':
  main()
