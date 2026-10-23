from __future__ import annotations

import contextlib
import io
from can_source import load, snapshot
from card_qa.mazda.source import Settings, LogCapture


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.psa.interface import CarInterface
  from opendbc.can import parser
  from opendbc.car.carlog import carlog
  settings = Settings(case['settings'])
  interfaces.Params = lambda: settings
  now = case['now']
  parser.time = type('Clock', (), {'monotonic_ns': staticmethod(lambda: now)})
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in case['fingerprints']})
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in case['firmware']]
  cp = CarInterface.get_params(case['candidate'], fingerprint, firmware, case['alpha_long'], True, False)
  if case['op']=='params': return {'params': list(cp.to_bytes()), 'writes': settings.writes}
  cp.carFw = firmware
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured): vehicle = CarInterface(cp)
  logs = LogCapture(); carlog.addHandler(logs)
  calls = []
  def recv(*args): calls.append(['recv']); return []
  def send(*args): calls.append(['send'])
  vehicle.init(cp, recv, send)
  rows = []
  failure = None
  try:
    for step in case['steps']:
      now = step['now']
      logs.rows.clear()
      if case['op']!='before_update':
        packets = [(p['mono_time'], [(f['address'], bytes(f['data']), f['bus']) for f in p['frames']]) for p in step['packets']]
        state = vehicle.update(packets)
      else: state = vehicle.CS.out
      vehicle.CS.softHoldActive = step['soft_hold']
      with structs.CarControl.from_bytes(bytes(step['control'])) as control:
        try: actuators, can = vehicle.apply(control, now)
        except (AttributeError, ValueError, OverflowError) as error:
          failure = {'kind': type(error).__name__, 'message': str(error)}
          break
      rows.append(dict(state=list(state.to_bytes()), actuators=list(actuators.to_bytes()),
        can=[dict(address=a, data=list(d), bus=b) for a,d,b in can],
        snapshot=dict(speed_filter=[float(v[0]) for v in vehicle.CS.v_ego_kf.x],
          cluster_seen=vehicle.v_ego_cluster_seen, eps_active=getattr(vehicle.CS,'eps_active',None),
          steering_pressed_count=vehicle.CS.steering_pressed_cnt),
        controller=dict(frame=vehicle.CC.frame, apply_angle_last=vehicle.CC.apply_angle_last, status=vehicle.CC.status),
        counters={str(a): str(v) for a,v in vehicle.CC.packer.counters.items()},
        parsers=[snapshot(p,set(),[]) for p in vehicle.can_parsers.values()], logs=logs.rows[:], soft_hold=vehicle.CS.softHoldActive))
  finally:
    vehicle.deinit(cp, recv, send); carlog.removeHandler(logs)
  return dict(params=list(cp.to_bytes()), writes=settings.writes, steps=rows, error=failure,
    lifecycle=calls, prints=captured.getvalue().splitlines())
