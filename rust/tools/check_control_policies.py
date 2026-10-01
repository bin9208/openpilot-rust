#!/usr/bin/env python3
import argparse
from contextlib import redirect_stdout
import difflib
import io
import json
import os
from pathlib import Path
import random
import subprocess
from openpilot.cereal import car

from check_paramsd import compare
from controlsd_parameters import Store
from controlsd_source import load


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  source, _, _, _ = load(Store())
  from opendbc.car.interfaces import get_nn_model_path, TORQUE_NN_MODEL_PATH, NEURAL_PARAMS_PATH

  neural = json.loads(Path(NEURAL_PARAMS_PATH).read_text())
  request = {'cases': [], 'files': os.listdir(TORQUE_NN_MODEL_PATH), 'selection': [], 'similarity': [], 'neural_keys': list(neural)}
  expected = {'cases': [], 'selection': [], 'similarity': []}
  for fingerprint, interface in sorted(source['interfaces'].items()):
    instance = interface.__new__(interface)
    instance.CP = car.CarParams.new_message(carFingerprint=fingerprint, flags=0)
    torque = instance.torque_from_lateral_accel().__name__
    feedforward = instance.get_steer_feedforward_function()
    for flags in [0, 1024, 65535]:
      instance.CP.flags = flags
      for speed in [-1.0, 0.0, 18.0, 19.6, 19.8, 20.0, 40.0]:
        for angle in [-30.0, -0.01, 0.0, 0.01, 30.0]:
          request['cases'].append({'fingerprint': fingerprint, 'flags': flags, 'speed': speed, 'cruise': 20.0, 'angle': angle})
          expected['cases'].append(
            {'limits': list(interface.get_pid_accel_limits(instance.CP, speed, 20.0)), 'feedforward': float(feedforward(angle, speed)), 'torque': torque}
          )
    for firmware in ['', "b'1234'", "b'\\xf1\\x00 model v1'", 'a' * 240]:
      request['selection'].append({'fingerprint': fingerprint, 'firmware': firmware})
      with redirect_stdout(io.StringIO()):
        selected = get_nn_model_path(fingerprint, firmware)
      expected['selection'].append(None if selected is None else Path(selected).name)
  rng = random.Random(150)
  for length in [0, 1, 20, 199, 200, 240, 500]:
    for _ in range(40):
      left = ''.join(rng.choices('aabc12_한글', k=length))
      right = left[: length // 2] + ''.join(rng.choices('aaad45_글', k=length // 2))
      request['similarity'].append([left, right])
      expected['similarity'].append(difflib.SequenceMatcher(None, left, right).ratio())
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  output = args.evidence / 'native.json'
  child = subprocess.run([args.trace.resolve(), output.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  errors = []
  compare(expected, actual, 'policies', errors)
  result = {
    'pass': True,
    'identities': len(source['interfaces']),
    'policy_cases': len(request['cases']),
    'model_selections': len(request['selection']),
    'sequence_pairs': len(request['similarity']),
    'max_error': max(errors),
  }
  (args.evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
