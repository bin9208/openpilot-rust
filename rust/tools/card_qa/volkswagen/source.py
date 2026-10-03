from __future__ import annotations

import contextlib
import copy
import io
from can_source import load, snapshot as parser_snapshot
from card_qa.mazda.source import Settings, LogCapture


EXTRAS = ('frame', 'eps_init_complete', 'esp_hold_confirmation', 'long_control_inhibit', 'upscale_lead_car_signal',
          'eps_stock_values', 'klr_stock_values', 'ea_hud_stock_values', 'ea_control_stock_values',
          'travel_assist_available', 'left_blinker_active', 'right_blinker_active', 'curvature', 'cruise_recovery_timer',
          'ldw_stock_values', 'gra_stock_values', 'acc_type', 'steering_pressed_cnt', 'left_blinker_cnt', 'right_blinker_cnt',
          'left_blinker_prev', 'right_blinker_prev')
HISTORY = ('frame', 'apply_torque_last', 'apply_curvature_last', 'steering_power_last', 'gra_acc_counter_last',
           'eps_timer_soft_disable_alert', 'hca_frame_timer_running', 'hca_frame_same_torque', 'long_override_counter',
           'hold_release_frames', 'long_disabled_counter', 'klr_counter_last', 'navi_event_last', 'navi_banner_frames',
           'road_limit_last', 'road_banner_frames', 'lead_limit_disp', 'lead_limit_cnt', 'acc_hold_type_last')


def extras(vehicle):
  from opendbc.car import structs
  values = {key: copy.deepcopy(getattr(vehicle.CS, key, None)) for key in EXTRAS}
  names = {value: key for key, value in structs.CarState.ButtonEvent.Type.schema.enumerants.items()}
  values['button_states'] = {names[key]: bool(value) for key, value in vehicle.CS.button_states.items()}
  return values


def controller(vehicle):
  return {key: bool(getattr(vehicle.CC, key)) if key in ('eps_timer_soft_disable_alert', 'lead_limit_disp') else getattr(vehicle.CC, key) for key in HISTORY}


def snapshot(vehicle):
  return {'speed_filter': [float(row[0]) for row in vehicle.CS.v_ego_kf.x], 'cluster_seen': vehicle.v_ego_cluster_seen}


def parsers(vehicle):
  from opendbc.car import Bus
  return [parser_snapshot(vehicle.can_parsers[bus], [], []) for bus in (Bus.pt, Bus.cam)]


def parameters(case, settings):
  from opendbc.car import interfaces
  from opendbc.car.volkswagen.interface import CarInterface
  interfaces.Params = lambda: settings
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in case['fingerprints']})
  return CarInterface.get_params(case['candidate'], fingerprint, [], case['alpha_long'], True, False)


def trace(case):
  load()
  from opendbc.car import structs
  from opendbc.car.volkswagen.interface import CarInterface
  from opendbc.can import parser
  from opendbc.car.carlog import carlog
  settings = Settings(case['settings'])
  now = case['now']
  parser.time = type('Clock', (), {'monotonic_ns': staticmethod(lambda: now)})
  prints = io.StringIO()
  with contextlib.redirect_stdout(prints):
    cp = parameters(case, settings)
  result = {'parameter_prints': prints.getvalue().splitlines()}
  if case['op'] == 'params':
    return {**result, 'params': list(cp.to_bytes()), 'writes': settings.writes}
  cp.flags |= case.get('flags_or', 0)
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured):
    vehicle = CarInterface(cp)
  if 'seed_state' in case:
    with structs.CarState.from_bytes(bytes(case['seed_state'])) as state:
      vehicle.CS.out = state.as_builder()
  for key, value in case.get('seed_extra', {}).items():
    if key != 'button_states':
      setattr(vehicle.CS, key, copy.deepcopy(value))
  for key, value in case.get('seed_history', {}).items():
    setattr(vehicle.CC, key, value)
  result.update(common={'use_nnff': vehicle.use_nnff, 'use_nnff_lite': vehicle.use_nnff_lite, 'model_present': vehicle.lat_torque_nn_model is not None},
                initial_state=list(vehicle.CS.out.to_bytes()), initial_extra=extras(vehicle), initial_state_snapshot=snapshot(vehicle),
                initial_controller=controller(vehicle), initial_packer_counters={str(k): str(v) for k, v in vehicle.CC.packer_pt.counters.items()},
                initial_parsers=parsers(vehicle))
  logs = LogCapture()
  carlog.addHandler(logs)
  lifecycle = []
  def recv(*args, **kwargs):
    lifecycle.append('receive')
    return []
  def send(*args, **kwargs):
    lifecycle.append('send')
  vehicle.init(cp, recv, send)
  steps, error = [], None
  for step in case['steps']:
    now = step['now']
    if case['op'] not in ('before_update', 'seeded_controller') and 'seed_state' not in case:
      packets = [(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet['frames']]) for packet in step['packets']]
      try:
        state = vehicle.update(packets)
      except NameError as failure:
        if case['op'] != 'mqb_failure':
          raise
        error = {'kind': type(failure).__name__, 'message': str(failure)}
        break
    else:
      state = vehicle.CS.out.as_reader().as_builder()
    if case['op'] != 'before_update':
      vehicle.CS.softHoldActive = step['soft_hold']
      for key, value in step['commit'].items():
        setattr(state, key, value)
      vehicle.CS.out = state
    with structs.CarControl.from_bytes(bytes(step['control'])) as control, contextlib.redirect_stdout(captured):
      try:
        actuators, can = vehicle.apply(control, now)
      except (AttributeError, ValueError, OverflowError, TypeError) as failure:
        if case['op'] not in ('numeric_error', 'before_update'):
          raise
        error = {'kind': type(failure).__name__, 'message': str(failure)}
        break
    steps.append({'state': list(state.to_bytes()), 'actuators': list(actuators.to_bytes()),
                  'can': [{'address': address, 'data': list(data), 'bus': bus} for address, data, bus in can],
                  'extra': extras(vehicle), 'state_snapshot': snapshot(vehicle), 'controller': controller(vehicle),
                  'packer_counters': {str(k): str(v) for k, v in vehicle.CC.packer_pt.counters.items()},
                  'logs': logs.rows[:], 'soft_hold': vehicle.CS.softHoldActive, 'is_metric': vehicle.CS.is_metric})
    logs.rows.clear()
  vehicle.deinit(cp, recv, send)
  carlog.removeHandler(logs)
  return {**result, 'params': list(cp.to_bytes()), 'writes': settings.writes, 'steps': steps, 'prints': captured.getvalue().splitlines(),
          'error': error, 'lifecycle': lifecycle, 'final_extra': extras(vehicle), 'final_state_snapshot': snapshot(vehicle), 'final_controller': controller(vehicle),
          'final_packer_counters': {str(k): str(v) for k, v in vehicle.CC.packer_pt.counters.items()}, 'final_parsers': parsers(vehicle),
          'final_state': list(vehicle.CS.out.to_bytes()), 'final_logs': logs.rows[:]}
