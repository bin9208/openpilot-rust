from __future__ import annotations

import contextlib
import copy
import io
from can_source import load
from card_qa.mazda.source import Settings, LogCapture


def extras(vehicle):
  names = ('brake_switch_prev', 'brake_switch_active', 'cruise_setting', 'cruise_buttons', 'v_cruise_pcm_prev',
           'dash_speed_seen', 'acc_hud', 'lkas_hud', 'stock_brake', 'left_blinker_cnt', 'right_blinker_cnt',
           'left_blinker_prev', 'right_blinker_prev')
  return {key: copy.deepcopy(getattr(vehicle.CS, key, None)) for key in names}


def controller(vehicle):
  names = ('frame', 'braking', 'brake_steady', 'brake_last', 'apply_brake_last', 'last_pump_ts',
           'stopping_counter', 'accel', 'speed', 'gas', 'brake', 'last_torque')
  return {'history': {key: bool(getattr(vehicle.CC, key)) if key == 'braking' else getattr(vehicle.CC, key) for key in names},
          'limits': {key.lower(): getattr(vehicle.CC.params, key) for key in
                     ('STEER_MAX', 'STEER_DELTA_UP', 'STEER_DELTA_DOWN', 'STEER_LOOKUP_BP', 'STEER_LOOKUP_V')}}


def snapshot(vehicle):
  return {'speed_filter': [float(row[0]) for row in vehicle.CS.v_ego_kf.x], 'cluster_seen': vehicle.v_ego_cluster_seen}


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.honda import carcontroller
  from opendbc.car.honda.interface import CarInterface
  from opendbc.can import parser
  from opendbc.car.carlog import carlog
  settings = Settings(case['settings'])
  interfaces.Params = carcontroller.Params = lambda: settings
  now = case['now']
  parser.time = type('Clock', (), {'monotonic_ns': staticmethod(lambda: now)})
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in case['firmware']]
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in case['fingerprints']})
  prints = io.StringIO()
  with contextlib.redirect_stdout(prints):
    cp = CarInterface.get_params(case['candidate'], fingerprint, firmware, case['alpha_long'], True, False)
  result = {'parameter_prints': prints.getvalue().splitlines()}
  if case['op'] == 'params':
    return {**result, 'params': list(cp.to_bytes()), 'writes': settings.writes}
  cp.flags |= case.get('flags_or', 0)
  if case.get('offset', False):
    cp.safetyConfigs = [{'safetyModel': 'noOutput'}, {}]
  cp.carFw = firmware
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured):
    vehicle = CarInterface(cp)
  result.update(common={'use_nnff': vehicle.use_nnff, 'use_nnff_lite': vehicle.use_nnff_lite,
                        'model_present': vehicle.lat_torque_nn_model is not None},
                initial_state=list(vehicle.CS.out.to_bytes()), initial_extra=extras(vehicle),
                initial_state_snapshot=snapshot(vehicle), initial_controller=controller(vehicle),
                initial_packer_counters={str(k): str(v) for k, v in vehicle.CC.packer.counters.items()})
  logs = LogCapture()
  carlog.addHandler(logs)
  calls, incoming, sent = [], [], []

  def recv(*args, **kwargs):
    calls.append(['recv'])
    result = incoming[:]
    incoming.clear()
    return result

  def send(frames):
    from opendbc.car.can_definitions import CanData
    calls.append(['send'])
    for address, data, bus in frames:
      sent.append({'address': address, 'data': list(data), 'bus': bus})
      assert address == 0x18dab0f1
      if data[:3] == b'\x02\x10\x03':
        incoming.append([CanData(0x18daf1b0, b'\x02\x50\x03\0\0\0\0\0', bus)])
      elif data[:4] == b'\x03\x28\x83\x03':
        incoming.append([CanData(0x18daf1b0, b'\x03\x68\x83\x03\0\0\0\0', bus)])
      else:
        raise AssertionError('unexpected Honda lifecycle request')

  vehicle.init(cp, recv, send)
  lifecycle_logs = logs.rows[:]
  steps, error = [], None
  logs.rows.clear()
  for step in case['steps']:
    settings.values.update(step['settings'])
    now = step['now']
    if case['op'] != 'before_update':
      packets = [(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus']) for frame in packet['frames']]) for packet in step['packets']]
      logs.rows.clear()
      state = vehicle.update(packets)
      vehicle.CS.softHoldActive = step['soft_hold']
      for key, value in step['commit'].items():
        setattr(state, key, value)
      vehicle.CS.out = state
    with structs.CarControl.from_bytes(bytes(step['control'])) as control, contextlib.redirect_stdout(captured):
      try:
        actuators, can = vehicle.apply(control, now)
      except (AttributeError, TypeError, ValueError, OverflowError) as failure:
        if case['op'] == 'runtime':
          raise
        error = {'kind': type(failure).__name__, 'message': str(failure)}
        break
      else:
        if case['op'] != 'runtime':
          raise AssertionError('Honda error fixture unexpectedly succeeded')
    steps.append({'state': list(state.to_bytes()), 'actuators': list(actuators.to_bytes()),
                  'can': [{'address': address, 'data': list(data), 'bus': bus} for address, data, bus in can],
                  'extra': extras(vehicle), 'state_snapshot': snapshot(vehicle), 'controller': controller(vehicle),
                  'packer_counters': {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()},
                  'logs': logs.rows[:], 'soft_hold': vehicle.CS.softHoldActive, 'is_metric': vehicle.CS.is_metric})
    logs.rows.clear()
  final_logs = logs.rows[:]
  vehicle.deinit(cp, recv, send)
  carlog.removeHandler(logs)
  return {**result, 'params': list(cp.to_bytes()), 'writes': settings.writes, 'steps': steps,
          'lifecycle': calls, 'lifecycle_can': sent, 'lifecycle_logs': lifecycle_logs, 'prints': captured.getvalue().splitlines(),
          'pre_update_error': error, 'final_extra': extras(vehicle), 'final_state_snapshot': snapshot(vehicle),
          'final_controller': controller(vehicle), 'final_packer_counters': {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()},
          'final_logs': final_logs}
