#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import struct
import subprocess

import numpy as np
from openpilot.cereal import car, log
from paramsd_fixture import scenarios
from paramsd_source import load


def values(raw):
  return [struct.unpack('<d', struct.pack('<Q', value))[0] for value in raw]


def compare(expected, actual, path, errors):
  match expected:
    case dict():
      assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
      for key in expected:
        compare(expected[key], actual[key], path + '.' + key, errors)
    case list():
      assert len(expected) == len(actual), (path, len(expected), len(actual))
      for i, (left, right) in enumerate(zip(expected, actual, strict=True)):
        compare(left, right, f'{path}[{i}]', errors)
    case float():
      tolerance = 2e-6 if '.event.' in path and '.debugFilterState.' not in path else 1e-9
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
  request = {'cases': scenarios()}
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  expected = []
  for case in request['cases']:
    with car.CarParams.from_bytes(bytes(case['car'])) as cp:
      learner = source['VehicleParamsLearner'](cp, cp.steerRatio, 1., 0.)
    rows = []
    for operation in case['operations']:
      if 'packet' in operation:
        with log.Event.from_bytes(bytes(operation['packet'])) as message:
          learner.handle_log(message.logMonoTime * 1e-9, message.which(), getattr(message, message.which()))
      if 'state' in operation:
        learner.kf.init_state(np.array(values(operation['state'])), covs=np.diag(values(operation['covariance'])), filter_time=operation['time'])
      message = learner.get_msg(True, debug=True)
      rows.append({'x': learner.kf.x.tolist(), 'p': learner.kf.P.flatten().tolist(), 'active': learner.active,
                   'time': None if np.isnan(learner.kf.t) else learner.kf.t,
                   'observed': [learner.observed_speed, learner.observed_yaw_rate, learner.observed_roll],
                   'event': message.to_dict(), 'logs': list(logs)})
      logs.clear()
    expected.append({'name': case['name'], 'rows': rows})
  (args.evidence / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
  output = args.evidence / 'native-raw.json'
  child = subprocess.run([args.trace.resolve(), output.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'native-process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  for case in actual:
    for row in case['rows']:
      row['x'], row['p'] = values(row.pop('x_bits')), values(row.pop('p_bits'))
      row['observed'] = values(row.pop('observed_bits'))
      with log.Event.from_bytes(bytes(row.pop('packet'))) as event:
        row['event'] = event.to_dict()
  (args.evidence / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
  errors = []
  compare(expected, actual, 'paramsd', errors)
  result = {'pass': True, 'cases': len(expected), 'steps': sum(len(case['rows']) for case in expected),
            'numeric_comparisons': len(errors), 'max_absolute_error': max(errors),
            'float64_tolerance': {'absolute': 1e-9, 'relative': 1e-9}, 'float32_tolerance': {'absolute': 2e-6, 'relative': 2e-6}}
  (args.evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
