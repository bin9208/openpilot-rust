from __future__ import annotations


def error_cases(profiles):
  from openpilot.cereal import car
  result = []
  for candidate in ('HONDA_CIVIC', 'HONDA_ACCORD', 'HONDA_CIVIC_2022', 'HONDA_ODYSSEY'):
    profile = next(case for case in profiles if case['candidate'] == candidate and case['alpha_long'])
    for number in ('nan', 'inf', '-inf'):
      for field in ('torque', 'accel', 'speed'):
        kwargs = {'actuators': {field: float(number)}} if field != 'speed' else {'hudControl': {'setSpeed': float(number), 'speedVisible': True}}
        control = car.CarControl.new_message(enabled=True, longActive=True, latActive=True, **kwargs)
        # Infinite torque/acceleration clip; NaN reaches an integer conversion or CAN packing boundary.
        error = number == 'nan' and field in ('torque', 'accel') or field == 'speed'
        result.append({**profile, 'name': f'{field}-{number}-{candidate}', 'op': 'numeric_error' if error else 'runtime',
                       'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}]})
    for key, value in (('CustomSteerMax', 'bad'), ('CustomSteerDeltaUp', '+'), ('CustomSteerDeltaDown', '2147483648')):
      settings = {'CustomSteerMax': '2400', 'CustomSteerDeltaUp': '99', 'CustomSteerDeltaDown': '99', key: value}
      result.append({**profile, 'name': f'setting-{key}-{candidate}', 'op': 'numeric_error', 'error_key': key,
                     'steps': [{**profile['steps'][0], 'settings': settings}]})
    for visible in (False, True):
      control = car.CarControl.new_message(enabled=True, latActive=True, longActive=True,
        hudControl={'setSpeed': 1e30 if visible else float('nan'), 'speedVisible': visible})
      result.append({**profile, 'name': f'hud-extreme-{candidate}-{visible}', 'op': 'runtime',
                     'steps': [{**profile['steps'][0], 'control': list(control.to_bytes())}]})
  return result
