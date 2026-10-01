#!/usr/bin/env python3
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

from openpilot.cereal import log
from check_locationd import compare
from locationd_loop_fixture import cases
from locationd_loop_source import trace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  request = {'cases': cases()}
  (args.evidence / 'loop-input.json').write_text(json.dumps(request) + '\n')
  expected = [trace(args.oracle.resolve(), case) for case in request['cases']]
  (args.evidence / 'loop-source.json').write_text(json.dumps(expected, indent=2) + '\n')
  output = args.evidence / 'loop-native-raw.json'
  child = subprocess.run([args.trace.resolve(), output.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'loop-native-process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  for case in actual:
    for row in case['rows']:
      row['publications'] = []
      for packet in row.pop('packets'):
        with log.Event.from_bytes(bytes(packet)) as event:
          row['publications'].append(event.to_dict())
  (args.evidence / 'loop-native.json').write_text(json.dumps(actual, indent=2) + '\n')
  errors = []
  compare(expected, actual, 'loop', errors)
  outcomes = Counter()
  for case in actual:
    for row in case['rows']:
      outcomes[f'initialized:{row["initialized"]}'] += 1
      for event in row['publications']:
        for key in ('inputsOK', 'sensorsOK', 'posenetOK'):
          outcomes[f'{key}:{event["livePose"][key]}'] += 1
  assert outcomes['initialized:True'] and outcomes['initialized:False']
  assert outcomes['inputsOK:False'] and outcomes['inputsOK:True']
  assert outcomes['sensorsOK:False'] and outcomes['sensorsOK:True']
  result = {'pass': True, 'cases': len(actual), 'frames': sum(len(case['rows']) for case in actual), 'max_absolute_error': max(errors), 'outcomes': outcomes}
  (args.evidence / 'loop-results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
