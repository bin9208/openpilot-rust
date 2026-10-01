#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess

from openpilot.cereal import log
from check_paramsd import compare, values
from paramsd_loop_fixture import cases
from paramsd_loop_source import trace


def normalize(item):
  for record in [item['init'], *item['rows']]:
    for entry in record['logs']:
      entry[1] = entry[1].split(':')[0] if entry[0] == 'error' else entry[1]
    if 'operations' in record:
      for action in record['operations']:
        if action[0] == 'put':
          with log.Event.from_bytes(bytes(action[2])) as message:
            action[2] = message.to_dict()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = {'cases': cases()}
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  expected = [trace(args.oracle.resolve(), case) for case in request['cases']]
  (args.evidence / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
  path = args.evidence / 'native-raw.json'
  child = subprocess.run([args.trace.resolve(), path.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(path.read_text())
  for item in actual:
    initial = item['init']
    initial['initial'] = values(initial.pop('initial_bits'))
    covariance = initial.pop('covariance_bits')
    initial['covariance'] = None if covariance is None else values(covariance)
    for row in item['rows']:
      row['x'], row['p'] = values(row.pop('x_bits')), values(row.pop('p_bits'))
      packet = row.pop('packet')
      row['event'] = None
      if packet is not None:
        with log.Event.from_bytes(bytes(packet)) as message:
          row['event'] = message.to_dict()
  (args.evidence / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
  for item in expected + actual:
    normalize(item)
  errors = []
  compare(expected, actual, 'loop', errors)
  rows = [row for item in actual for row in item['rows']]
  result = {'pass': True, 'cases': len(actual), 'frames': len(rows), 'saves': sum(row['cache'] for row in rows),
            'gps_writes': sum(row['gps'] is not None for row in rows), 'max_absolute_error': max(errors)}
  assert result['saves'] == 4 and result['gps_writes'] > 0
  (args.evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
