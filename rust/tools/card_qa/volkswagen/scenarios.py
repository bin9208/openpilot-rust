from __future__ import annotations

import contextlib
import io
import itertools
from can_source import load
from card_qa.mazda.source import Settings
from card_qa.volkswagen.source import extras, controller, parameters
from card_qa.volkswagen.frames import runtime_steps, control


def profile(candidate, alpha=False, gateway=False, detected=0, settings=None):
  from opendbc.car.volkswagen.values import VolkswagenFlags as F
  flags = candidate.config.flags
  meb, pq = bool(flags & F.MEB), bool(flags & F.PQ)
  pt = ([0x24c, 0x25d, 0x3dc, 0x24f] if meb else [0x3ba, 0x440] if pq else [0x30f, 0xad]) if detected == 1 else []
  if detected == 2 and not meb and not pq:
    pt = [0x187]
  camera = [0x1a4, 0x1f0] if meb and detected == 1 else [0x126] if not pq and not meb and detected == 1 else []
  gw = [0x520 if meb else 0x1a0 if pq else 0x86] if gateway else []
  return {'name': f'{candidate}-{alpha}-{gateway}-{detected}', 'op': 'params', 'candidate': str(candidate),
          'alpha_long': alpha, 'fingerprints': [[0, [[a, 8] for a in pt]], [1, [[a, 8] for a in gw]], [2, [[a, 8] for a in camera]]],
          'firmware': [], 'settings': settings or {}, 'now': 2_000_000_000, 'steps': []}


def seeded(profile_):
  from opendbc.car.volkswagen.interface import CarInterface
  from openpilot.cereal import car
  with contextlib.redirect_stdout(io.StringIO()):
    cp = parameters(profile_, Settings({}))
    vehicle = CarInterface(cp)
  state = car.CarState.new_message(vEgo=15., vEgoRaw=15., steeringTorque=-81., accFaulted=False,
    cruiseState={'available': True, 'enabled': True}, gasPressed=False)
  extra = extras(vehicle)
  dbc = vehicle.CC.packer_pt.dbc
  values = lambda name: {key: 0. for key in dbc.name_to_msg[name].sigs}
  extra.update(acc_type=1., esp_hold_confirmation=True, upscale_lead_car_signal=True,
               eps_stock_values=values('LH_EPS_03'), ldw_stock_values=values('LDW_02'), gra_stock_values=values('GRA_ACC_01'))
  history = controller(vehicle)
  history.update(frame=34998, hca_frame_timer_running=34998, hca_frame_same_torque=190, apply_torque_last=4)
  steps = runtime_steps(profile_, 260)
  from openpilot.cereal import car
  cc = car.CarControl.new_message(enabled=True, latActive=True, longActive=True, actuators={'torque': 4. / 300.})
  for step in steps[:3]:
    step['control'] = list(cc.to_bytes())
  return {**profile_, 'op': 'seeded_controller', 'name': f"seeded-controller-{profile_['name']}",
          'seed_state': list(state.to_bytes()), 'seed_extra': extra, 'seed_history': history,
          'steps': steps}


def cases(op=None, candidate_filter=None):
  load()
  from opendbc.car.volkswagen.values import CAR, VolkswagenFlags as F
  from openpilot.cereal import car
  result = []
  selected = [candidate for candidate in CAR if candidate_filter is None or str(candidate) == candidate_filter]
  for candidate, alpha, gateway, detected, settings in itertools.product(selected if op in (None, 'params') else [], (False, True), (False, True), range(3), ({}, {'NNFF': '1', 'NNFFLite': '1', 'DisableMinSteerSpeed': '1'})):
    case = profile(candidate, alpha, gateway, detected, settings)
    case['name'] = f"params-{case['name']}-{bool(settings)}"
    result.append(case)
  for candidate in selected:
    if not candidate.config.flags & (F.PQ | F.MEB):
      if op in (None, 'before_update'):
        for alpha, gateway, detected in itertools.product((False, True), (False, True), (0, 1)):
          case = profile(candidate, alpha, gateway, detected)
          case.update(op='before_update', name=f"before-update-{case['name']}", steps=runtime_steps(case, 1))
          result.append(case)
      for alpha, gateway, detected in itertools.product((False, True), (False, True), range(3) if op in (None, 'mqb_failure') else []):
        case = profile(candidate, alpha, gateway, detected)
        case.update(op='mqb_failure', name=f"mqb-failure-{case['name']}", steps=runtime_steps(case, 1))
        result.append(case)
      for alpha, gateway in itertools.product((False, True), (False, True) if op in (None, 'seeded_controller') else []):
        result.append(seeded(profile(candidate, alpha, gateway, 1)))
    else:
      for alpha, gateway, detected in itertools.product((False, True), (False, True), (0, 1) if op in (None, 'runtime', 'before_update') else []):
        case = profile(candidate, alpha, gateway, detected)
        case.update(op='runtime', name=f"runtime-{case['name']}", steps=runtime_steps(case, 1 if op == 'before_update' else 660))
        result.append(case)
        result.append({**case, 'op': 'before_update', 'name': f"before-update-{case['name']}", 'steps': [case['steps'][0]]})
  for candidate in (CAR.VOLKSWAGEN_PASSAT_NMS, CAR.VOLKSWAGEN_ID4_MK1, CAR.VOLKSWAGEN_ID4_MK2):
    if op not in (None, 'runtime', 'numeric_error') or candidate_filter is not None and str(candidate) != candidate_filter:
      continue
    case = profile(candidate, True, True, 1)
    step = runtime_steps(case, 1, quiet=True)[0]
    for number, field in itertools.product(('nan', 'inf', '-inf', '1e30'), ('torque', 'curvature', 'accel', 'speed')):
      kwargs = {'actuators': {field: float(number)}} if field in ('torque', 'curvature', 'accel') else {'hudControl': {'setSpeed' if field == 'speed' else 'naviEventSpeed': float(number), 'naviEventType': 8}}
      cc = car.CarControl.new_message(enabled=True, latActive=True, longActive=True, **kwargs)
      error = (field == 'torque' and not candidate.config.flags & F.MEB and number in ('nan', 'inf', '-inf') or
               field == 'curvature' and candidate.config.flags & F.MEB and number == 'nan' or
               field == 'accel' and number == 'nan' or field == 'speed' and (not candidate.config.flags & F.MEB and number in ('nan', 'inf', '-inf') or candidate.config.flags & F.MEB and number == '-inf') or
               False)
      result.append({**case, 'op': 'numeric_error' if error else 'runtime', 'name': f'numeric-{candidate}-{field}-{number}', 'steps': [{**step, 'control': list(cc.to_bytes())}]})
  if op in (None, 'numeric_error', 'before_update') and candidate_filter is None:
    from card_qa.volkswagen.edges import error_cases
    result.extend(error_cases(profile, seeded, runtime_steps))
  if op in (None, 'runtime'):
    for candidate in (CAR.VOLKSWAGEN_ID4_MK1, CAR.VOLKSWAGEN_ID4_MK2):
      if candidate_filter is not None and str(candidate) != candidate_filter:
        continue
      case = profile(candidate, True, True, 1)
      case.update(op='runtime', name=f'focused-hold-eps-navi-{candidate}', steps=runtime_steps(case, 1100, quiet=True, focused=True))
      result.append(case)
  if op in (None, 'seeded_controller'):
    from card_qa.volkswagen.edges import hold_boundaries
    result.extend(case for case in hold_boundaries(profile, runtime_steps) if candidate_filter is None or case['candidate'] == candidate_filter)
  return result
