from __future__ import annotations


def error_cases(profile, seeded, runtime_steps):
  from opendbc.car.volkswagen.values import CAR
  result = []
  pq = profile(CAR.VOLKSWAGEN_PASSAT_NMS)
  result.append({**pq, 'op': 'before_update', 'name': 'inherited-pq-missing-eps-relay', 'flags_or': 1, 'steps': runtime_steps(pq, 1)})
  mqb = seeded(profile(CAR.VOLKSWAGEN_GOLF_MK7, True, True, 1))
  for flag, attribute, value, name in [(64, 'klr_stock_values', {'COUNTER': 1.}, 'wheel-touch'), (16384, 'ea_hud_stock_values', {'EA_Blinken': 0.}, 'ea-hud')]:
    result.append({**mqb, 'op': 'numeric_error', 'name': f'inherited-mqb-missing-{name}', 'flags_or': flag,
                   'seed_extra': {**mqb['seed_extra'], attribute: value}, 'steps': [mqb['steps'][0]]})
  return result


def hold_boundaries(profile, runtime_steps):
  import contextlib
  import io
  from card_qa.mazda.source import Settings
  from card_qa.volkswagen.source import parameters, extras, controller
  from opendbc.car.volkswagen.interface import CarInterface
  from opendbc.car.volkswagen.values import CAR
  from openpilot.cereal import car
  result = []
  for candidate in (CAR.VOLKSWAGEN_ID4_MK1, CAR.VOLKSWAGEN_ID4_MK2):
    case = profile(candidate, True, True, 1)
    with contextlib.redirect_stdout(io.StringIO()):
      vehicle = CarInterface(parameters(case, Settings({})))
      vehicle.update([])
    history = controller(vehicle)
    history['hold_release_frames'] = 20
    extra = extras(vehicle)
    extra.update(esp_hold_confirmation=False, long_control_inhibit=False,
                 left_blinker_prev=bool(extra['left_blinker_prev']), right_blinker_prev=bool(extra['right_blinker_prev']))
    state = car.CarState.new_message(vEgo=0.3, vEgoRaw=0.3, cruiseState={'available': True})
    cc = car.CarControl.new_message(enabled=True, longActive=True, actuators={'accel': 0.3, 'longControlState': 'pid'})
    step = runtime_steps(case, 1, quiet=True)[0]
    step['control'] = list(cc.to_bytes())
    result.append({**case, 'op': 'seeded_controller', 'name': f'seeded-meb-done-speed-f32-{candidate}',
                   'seed_state': list(state.to_bytes()), 'seed_extra': extra, 'seed_history': history, 'steps': [step]})
  return result
