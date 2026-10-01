#!/usr/bin/env python3
import argparse
import copy
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess

from openpilot.cereal import car
from controlsd_fixture import cases
from controlsd_source import trace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  basic = cases()
  fixtures = []
  for key, value in [
    ('StoppingAccel', b'not a number'),
    ('SteerRatioRate', b'1e999'),
    ('CustomSR', b'bad'),
    ('LatSuspendAngleDeg', b'2147483648'),
    ('SpeedFromPCM', b'x'),
    ('DisableDM', b'x'),
  ]:
    case = copy.deepcopy(basic[0])
    case['frames'] = case['frames'][:5]
    case['name'] = 'malformed-' + key
    case['params'][key] = list(value)
    fixtures.append(case)
  for value in [b'', b'{', b'{1: {}}', b'\xff'] + [
    ('{0: {' + token + ': 8}, 1: {}, 2: {}, 3: {}}').encode() for token in ['1_', '_1', '1__0', '0x__1', '0x1_', '0b2', '0o8', '01', '0_1', '0x', '++1']
  ]:
    case = copy.deepcopy(basic[3])
    case['frames'] = case['frames'][:1]
    case['name'] = 'fingerprints-' + value.hex()
    case['params']['FingerPrints'] = list(value)
    fixtures.append(case)
  for name, mutate in [
    ('unknown-fingerprint', lambda cp: setattr(cp, 'carFingerprint', 'MISSING')),
    ('zero-wheelbase', lambda cp: setattr(cp, 'wheelbase', 0.0)),
  ]:
    case = copy.deepcopy(basic[0])
    case['frames'] = case['frames'][:5]
    case['name'] = name
    with car.CarParams.from_bytes(bytes(case['params']['CarParams'])) as original:
      cp = original.as_builder()
      mutate(cp)
      case['params']['CarParams'] = list(cp.to_bytes())
    fixtures.append(case)
  case = copy.deepcopy(basic[0])
  case['name'] = 'psa-valid-tuning'
  case['frames'] = case['frames'][:5]
  with car.CarParams.from_bytes(bytes(case['params']['CarParams'])) as original:
    cp = original.as_builder()
    cp.carFingerprint = 'PSA_PEUGEOT_208'
    cp.brand = 'psa'
    case['params']['CarParams'] = list(cp.to_bytes())
  fixtures.append(case)
  case = copy.deepcopy(basic[1])
  case['name'] = 'zero-torque-factor'
  case['frames'] = case['frames'][:5]
  with car.CarParams.from_bytes(bytes(case['params']['CarParams'])) as original:
    cp = original.as_builder()
    cp.lateralTuning.torque.latAccelFactor = 0.0
    case['params']['CarParams'] = list(cp.to_bytes())
  from controlsd_scenarios import change

  case['frames'] = [change(row, {'liveTorqueParameters': {'useParams': False}}) for row in case['frames']]
  fixtures.append(case)
  case = copy.deepcopy(basic[1])
  case['name'] = 'angle-with-torque-live-update'
  case['simulation'] = True
  case['frames'] = case['frames'][:5]
  with car.CarParams.from_bytes(bytes(case['params']['CarParams'])) as original:
    cp = original.as_builder()
    cp.steerControlType = 'angle'
    case['params']['CarParams'] = list(cp.to_bytes())
  fixtures.append(case)
  case = copy.deepcopy(basic[0])
  case['name'] = 'truncated-CarParams'
  case['params']['CarParams'] = [0, 1, 2]
  case['frames'] = []
  fixtures.append(case)
  results = []
  for case in fixtures:
    output = args.evidence / case['name']
    output.mkdir(exist_ok=True)
    request = {'cases': [case]}
    (output / 'input.json').write_text(json.dumps(request) + '\n')
    printed = io.StringIO()
    try:
      with redirect_stdout(printed):
        trace(case)
    except Exception as error:
      source = {'exception': type(error).__name__, 'message': str(error)}
    else:
      raise AssertionError(('source accepted malformed case', case['name']))
    (output / 'source.json').write_text(json.dumps(source, indent=2) + '\n')
    child = subprocess.run(
      [args.trace.resolve(), (output / 'native.json').resolve()],
      input=json.dumps(request),
      text=True,
      capture_output=True,
      env=os.environ | {'CONTROLS_NUMERICS': str(args.numerics.resolve())},
    )
    (output / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
    assert child.returncode != 0 and not (output / 'native.json').exists(), case['name']
    results.append({'name': case['name'], 'source': source['exception'], 'native_exit': child.returncode, 'pass': True})
  (args.evidence / 'results.json').write_text(json.dumps({'pass': True, 'cases': results}, indent=2) + '\n')
  print(json.dumps({'pass': True, 'cases': len(results)}))


if __name__ == '__main__':
  main()
