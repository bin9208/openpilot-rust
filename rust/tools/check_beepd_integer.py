#!/usr/bin/env python3
"""Real Cython std::stoi and native parser differential, retaining source abort diagnostics."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from beep_fixtures import environment


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  values = [b'', b'0', b'5', b'6', b'-5', b'+6', b'6tail', b'6.5', b'6_000', b'0x10', b'6\0tail', b'\0',
            b'2147483647', b'2147483648', b'-2147483648', b'-2147483649', b'99999999999999999999999999999999',
            b'0000000000000000000000000000000000000006', b'  \v\f\t\r\n+6x', b'--1', b'+', b'-']
  values.extend(bytes([byte]) + b'6tail' for byte in range(256))
  results = []
  for index, value in enumerate(values):
    pair = []
    for implementation in ['python', 'rust']:
      with tempfile.TemporaryDirectory(prefix='beep-number-') as temporary, environment(Path(temporary), args.binding.resolve(), value) as (config, _):
        config.update(mode='integer', bytes=list(value))
        command = [sys.executable, str(Path(__file__).with_name('beep_source.py'))] if implementation == 'python' else [str(args.binary.resolve())]
        result = subprocess.run(command, input=json.dumps(config) + '\n', capture_output=True, text=True, timeout=5)
        if implementation == 'python' and result.returncode == -6:
          assert 'stoi' in result.stderr, result
          outcome = {'error': 'Range' if 'out_of_range' in result.stderr else 'Invalid'}
        else:
          assert result.returncode == 0, result
          outcome = json.loads(result.stdout)
        pair.append({'implementation': implementation, 'exit_code': result.returncode, 'outcome': outcome, 'stderr': result.stderr})
    assert pair[0]['outcome'] == pair[1]['outcome'], (index, value, pair)
    assert all('Failed to cast param' not in row['stderr'] for row in pair)
    results.append({'bytes_hex': value.hex(), 'results': pair})
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'cases': len(results), 'results': results}, indent=2) + '\n')
  print(f'PASS: {len(results)} real Cython/native integer cases, including all leading bytes and fatal ranges')


if __name__ == '__main__':
  main()
