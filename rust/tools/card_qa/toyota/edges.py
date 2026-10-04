from __future__ import annotations

import itertools


def numeric_cases(profile):
  from openpilot.cereal import car

  result = []
  for name, active in itertools.product(('nan', 'inf', '-inf'), (False, True)):
    control = car.CarControl.new_message(latActive=active, orientationNED=[0., -0.1, 0.], actuators={'torque': float(name)})
    result.append({**profile, 'name': f'torque-{name}-{active}', 'op': 'numeric_error',
                   'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}]})
  for active in (False, True):
    control = car.CarControl.new_message(latActive=active, longActive=True, orientationNED=[0., -0.1, 0.],
                                        actuators={'torque': 0.5, 'accel': float('nan'), 'longControlState': 'pid'})
    result.append({**profile, 'name': f'accel-nan-{active}', 'op': 'numeric_error',
                   'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}]})
    control = car.CarControl.new_message(latActive=True, longActive=active, orientationNED=[0., float('-inf'), 0.],
                                        actuators={'torque': 0.5, 'accel': 0.4, 'longControlState': 'pid'})
    result.append({**profile, 'name': f'pitch-negative-inf-{active}', 'op': 'numeric_error',
                   'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}]})
  for pitch in ('inf', 'nan'):
    control = car.CarControl.new_message(latActive=True, longActive=True, orientationNED=[0., float(pitch), 0.],
                                        actuators={'torque': 0.5, 'accel': 0.4, 'longControlState': 'pid'})
    result.append({**profile, 'name': f'pitch-{pitch}-source-continues', 'op': 'runtime',
                   'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}, *profile['steps'][1:4]]})
  for key, value in (('CustomSteerMax', 'bad'), ('CustomSteerDeltaUp', '+'), ('CustomSteerDeltaDown', '2147483648')):
    control = car.CarControl.new_message(latActive=True, orientationNED=[0., -0.1, 0.], actuators={'torque': 1.})
    settings = {'CustomSteerMax': '2400', 'CustomSteerDeltaUp': '99', 'CustomSteerDeltaDown': '99', key: value}
    result.append({**profile, 'name': f'setting-{key}-{value}', 'op': 'numeric_error', 'error_key': key,
                   'steps': [{**profile['steps'][0], 'control': list(control.to_bytes()), 'settings': settings}]})
  return result
