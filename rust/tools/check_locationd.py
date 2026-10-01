#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess

import numpy as np
from openpilot.cereal import log
from locationd_fixture import scenarios
from locationd_source import load


def compare(expected, actual, path, errors):
  match expected:
    case dict():
      assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
      for key in expected:
        compare(expected[key], actual[key], path + '.' + key, errors)
    case list():
      assert len(expected) == len(actual), (path, len(expected), len(actual))
      for i, (a, b) in enumerate(zip(expected, actual, strict=True)):
        compare(a, b, f'{path}[{i}]', errors)
    case float():
      event = '.event.' in path or '.publications[' in path
      tolerance = 2e-6 if event and '.debugFilterState.value' not in path and '.debugFilterState.std' not in path else 1e-9
      if '.invalid[' in path:
        tolerance = 1e-12
      assert np.isclose(expected, actual, atol=tolerance, rtol=tolerance, equal_nan=True), (path, expected, actual)
      if np.isfinite(expected) and np.isfinite(actual):
        errors.append(abs(expected - actual))
    case _:
      assert expected == actual, (path, expected, actual)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source, logs = load(args.oracle.resolve())
  cases = scenarios()
  request = {'cases': cases}
  (args.evidence / 'estimator-input.json').write_text(json.dumps(request) + '\n')
  expected = []
  for case in cases:
    estimator = source['LocationEstimator'](True)
    if 'reset_time' in case:
      estimator.reset(case['reset_time'])
    rows = []
    logs.clear()
    for packet in case['events']:
      with log.Event.from_bytes(bytes(packet)) as event:
        result = estimator.handle_log(event.logMonoTime * 1e-9, event.which(), getattr(event, event.which()))
      message = estimator.get_msg(True, True, True)
      rows.append(
        {'result': result.value, 'x': estimator.kf.x.tolist(), 'p': estimator.kf.P.flatten().tolist(), 'event': message.to_dict(), 'logs': list(logs)}
      )
      logs.clear()
    expected.append({'name': case['name'], 'rows': rows})
  (args.evidence / 'estimator-source.json').write_text(json.dumps(expected, indent=2) + '\n')
  output = args.evidence / 'estimator-native-raw.json'
  child = subprocess.run([args.trace.resolve(), output.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'estimator-native-process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  for case in actual:
    for row in case['rows']:
      with log.Event.from_bytes(bytes(row.pop('packet'))) as event:
        row['event'] = event.to_dict()
  (args.evidence / 'estimator-native.json').write_text(json.dumps(actual, indent=2) + '\n')
  errors = []
  compare(expected, actual, 'estimator', errors)
  result = {
    'pass': True,
    'cases': len(cases),
    'steps': sum(len(case['events']) for case in cases),
    'numeric_comparisons': len(errors),
    'max_absolute_error': max(errors),
    'float64_tolerance': {'absolute': 1e-9, 'relative': 1e-9},
    'float32_tolerance': {'absolute': 2e-6, 'relative': 2e-6},
  }
  (args.evidence / 'estimator-results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
