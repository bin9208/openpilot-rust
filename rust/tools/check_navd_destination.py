from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import sys


def cases():
  values = ['37.0,127.0', 'https://maps.google.com/x/@37.125,127.75,15z/data',
            'x/@0,1/y/@32.71,-117.12,17z/anything', '-0.0,0.0', '+NaN,-Infinity', '1e400,-1e-400',
            '\u2003３_７.０,١٢٧.٠\u00a0', '', 'single', 'bad,127', '37,bad', '37,,', '37,127/extra',
            '37,127,extra', '1__2,0', '_12,0', '12_,0', '++1,0', '1.2e+3,-4.2E-4', '0x10,12']
  randomizer = random.Random(196)
  values.extend(f'{randomizer.uniform(-90, 90):.17g},{randomizer.uniform(-180, 180):.17g}' for _ in range(50))
  output = [{'name': f'url-{index}', 'arguments': [value]} for index, value in enumerate(values)]
  output.extend([{'name': 'default', 'arguments': []}, {'name': 'extra-ignored', 'arguments': ['37,127', 'ignored']},
                 {'name': 'missing-waypoints', 'arguments': ['37,127'], 'waypoints': False},
                 {'name': 'destination-is-directory', 'arguments': ['37,127'], 'destination_directory': True},
                 {'name': 'waypoints-is-directory', 'arguments': ['37,127'], 'waypoints_directory': True}])
  return output


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument("--runner", nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  source = Path(__file__).with_name('navd_destination_source.py')
  results = []
  for case in cases():
    records = []
    for implementation in ('source', 'native'):
      output = args.output / case['name'] / implementation
      params = output / 'params/d'
      params.mkdir(parents=True)
      if case.get('destination_directory'):
        (params / 'NavDestination').mkdir()
      else:
        (params / 'NavDestination').write_bytes(b'original destination')
      if case.get('waypoints_directory'):
        (params / 'NavDestinationWaypoints').mkdir()
      elif case.get('waypoints', True):
        (params / 'NavDestinationWaypoints').write_bytes(b'original waypoints')
      env = dict(os.environ, PARAMS_ROOT=str(params.parent.resolve()))
      env.pop('OPENPILOT_PREFIX', None)
      command = ([sys.executable, str(source), '--binding', str(args.binding.resolve()), '--output', str(output.resolve())]
                 if implementation == 'source' else [*args.runner, str(args.binary.resolve())])
      if implementation == 'native':
        command.extend(case['arguments'])
      process = subprocess.run(command, input=json.dumps(case['arguments']).encode(), env=env,
                               capture_output=True, timeout=10, check=False)
      (output / 'stdout').write_bytes(process.stdout)
      (output / 'stderr').write_bytes(process.stderr)
      record = {'returncode': process.returncode, 'stdout': process.stdout.decode(),
                'parameters': {path.name: path.read_bytes().hex() if path.is_file() else '<directory>' for path in params.iterdir()}}
      (output / 'record.json').write_text(json.dumps(record, indent=2) + '\n')
      records.append(record)
    results.append({'name': case['name'], 'passed': records[0] == records[1]})
  root = Path(__file__).resolve().parents[2]
  files = [Path(__file__), source, args.binary, args.binding, root / 'openpilot/selfdrive/navd/set_destination.py',
           root / 'rust/crates/runtime-core/src/python_float.rs', *sorted((root / 'rust/crates/navd').rglob('*.rs'))]
  report = {'status': 'PASS' if all(row['passed'] for row in results) else 'FAIL', 'cases': results,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'status': report['status'], 'cases': len(results), 'failures': [row['name'] for row in results if not row['passed']]}))
  raise SystemExit(report['status'] != 'PASS')


if __name__ == '__main__':
  main()
