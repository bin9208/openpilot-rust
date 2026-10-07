#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import time
import uuid

from joystickd_source import SourceRun, load_binding


def bits(value: float) -> int:
  return struct.unpack('<I', struct.pack('<f', value))[0]


def snapshot(cc, cs) -> dict:
  control = cc.carControl
  assert cc.valid and cs.valid
  assert str(cs.controlsState.lateralControlState.which()) == 'debugState'
  assert cc.logMonoTime > 0 and cs.logMonoTime > 0
  return {'control': {
    'enabled': control.enabled, 'lat_active': control.latActive, 'long_active': control.longActive,
    'cancel': control.cruiseControl.cancel, 'resume': control.cruiseControl.resume,
    'lead_distance_bars': control.hudControl.leadDistanceBars,
    'long_state': str(control.actuators.longControlState),
    'actuators': [bits(v) for v in (control.actuators.accel, control.actuators.torque,
                                  control.actuators.steeringAngleDeg, control.actuators.curvature)]},
    'curvature': bits(cs.controlsState.curvature), 'error': None}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  os.environ['OPENPILOT_PREFIX'] = 'joystickd208_' + uuid.uuid4().hex
  os.environ['PARAMS_ROOT'] = str((args.output / 'params').resolve())
  os.environ['LOGPRINT'] = 'info'
  ipc_path = Path('/dev/shm') / ('msgq_' + os.environ['OPENPILOT_PREFIX'])
  ipc_path.mkdir()
  load_binding(args.binding.resolve())
  from openpilot.cereal import car, messaging
  from openpilot.tools.joystick import joystickd as original

  request = json.loads((args.source / 'input.json').read_bytes())[0]
  raw_cp = (args.source / '0/CarParams.bin').read_bytes()
  with car.CarParams.from_bytes(raw_cp) as value:
    cp = value.as_builder()
  base = request['inputs'][0] | {'frame': 100, 'joystick_frame': 99, 'speed': 5.0,
    'enabled': True, 'active': True, 'steer_fault_temporary': False, 'steer_fault_permanent': False,
    'override_longitudinal': False, 'cruise_enabled': True, 'axes': [0.5, 0.75]}
  rows = {
    'fresh': base,
    'expired': base | {'joystick_frame': 0},
    'recovered': base | {'axes': [-0.5, -0.75]},
    'override': base | {'steer_fault_temporary': True, 'override_longitudinal': True},
    'disabled': base | {'active': False, 'enabled': False},
  }
  expected = {}
  for name, row in rows.items():
    path = args.output / ('source-' + name)
    path.mkdir()
    expected[name] = SourceRun(original, cp, [row], path).run()[1][0]

  params_path = Path(os.environ['PARAMS_ROOT']) / os.environ['OPENPILOT_PREFIX']
  params_path.mkdir(parents=True)
  cp_path = params_path / 'CarParams'
  cp_path.mkdir()
  names = ['carState', 'onroadEvents', 'liveParameters', 'selfdriveState', 'testJoystick']
  publisher = messaging.PubMaster(names)
  cc_socket = messaging.sub_sock('carControl', timeout=2000)
  cs_socket = messaging.sub_sock('controlsState', timeout=2000)
  binary = str(args.binary.resolve())
  stdout = (args.output / 'daemon.stdout.log').open('w')
  stderr = (args.output / 'daemon.stderr.log').open('w')
  process = subprocess.Popen([binary, '--frames', '240'], stdout=stdout, stderr=stderr)
  records, matches = [], []

  def publish(row: dict) -> None:
    car_event = messaging.new_message('carState')
    cs = car_event.carState
    cs.vEgo, cs.steeringAngleDeg = row['speed'], row['steering_angle']
    cs.steerFaultTemporary, cs.steerFaultPermanent = row['steer_fault_temporary'], row['steer_fault_permanent']
    cs.cruiseState.enabled = row['cruise_enabled']
    drive = messaging.new_message('selfdriveState')
    drive.selfdriveState.enabled, drive.selfdriveState.active = row['enabled'], row['active']
    live = messaging.new_message('liveParameters')
    live.liveParameters.roll, live.liveParameters.angleOffsetDeg = row['roll'], row['angle_offset']
    events = messaging.new_message('onroadEvents', int(row['override_longitudinal']))
    if row['override_longitudinal']:
      events.onroadEvents[0].overrideLongitudinal = True
    joystick = messaging.new_message('testJoystick')
    joystick.testJoystick.axes = row['axes']
    for name, event in zip(names, [car_event, events, live, drive, joystick], strict=True):
      publisher.send(name, event)

  def receive() -> dict | None:
    cc, cs = None, None
    deadline = time.monotonic() + 2
    while cc is None or cs is None:
      if cc is None:
        cc = messaging.recv_one_or_none(cc_socket)
      if cs is None:
        cs = messaging.recv_one_or_none(cs_socket)
      if cc is None and cs is None and process.poll() is not None:
        return None
      assert time.monotonic() < deadline, 'publication timeout'
      if cc is None or cs is None:
        time.sleep(0.001)
    records.append({'carControl': cc.to_dict(), 'controlsState': cs.to_dict()})
    return snapshot(cc, cs)

  def match_phase(name: str) -> None:
    consecutive = 0
    for _ in range(80):
      result = receive()
      assert result is not None, 'daemon stopped during phase'
      consecutive = consecutive + 1 if result == expected[name] else 0
      if consecutive == 3:
        matches.append({'phase': name, 'publication': len(records), 'actual': result})
        return
    raise AssertionError(('phase did not match original', name, result, expected[name]))

  try:
    assert cc_socket.receive() is None, 'CarParams read error must wait without publications'
    assert process.poll() is None, 'CarParams read error must retry'
    cp_path.rmdir()
    cp_path.write_bytes(raw_cp)
    assert receive() is not None
    publish(rows['fresh'])
    match_phase('fresh')
    match_phase('expired')
    for name in ['recovered', 'override', 'disabled']:
      publish(rows[name])
      match_phase(name)
    while receive() is not None:
      pass
    assert len(records) >= 235, len(records)
    assert process.wait(timeout=3) == 0
  finally:
    if process.poll() is None:
      process.send_signal(signal.SIGTERM)
      process.wait(timeout=3)
    stdout.close()
    stderr.close()
    (args.output / 'publications.json').write_text(json.dumps(records) + '\n')
    (args.output / 'matches.json').write_text(json.dumps(matches, indent=2) + '\n')

  checks = {}
  cp_path.unlink()
  for name, value in [('waiting', None), ('malformed', b'bad cereal')]:
    if value is not None:
      cp_path.write_bytes(value)
    with (args.output / (name + '.stdout.log')).open('w') as out, (args.output / (name + '.stderr.log')).open('w') as err:
      child = subprocess.Popen([binary], stdout=out, stderr=err)
      if value is None:
        assert cc_socket.receive() is None
        assert child.poll() is None
        child.send_signal(signal.SIGTERM)
      code = child.wait(timeout=3)
      assert code == (0 if value is None else 1)
      checks[name] = code
  for values in (['--frames', '0'], ['--frames', '1', '--frames', '1'], ['--unknown']):
    result = subprocess.run([binary, *values], capture_output=True, check=False)
    assert result.returncode == 1
  cp_path.write_bytes(raw_cp)
  with (args.output / 'active-signal.stdout.log').open('w') as out, (args.output / 'active-signal.stderr.log').open('w') as err:
    child = subprocess.Popen([binary], stdout=out, stderr=err)
    try:
      active = None
      deadline = time.monotonic() + 2
      while active is None:
        active = messaging.recv_one_or_none(cc_socket)
        assert time.monotonic() < deadline and child.poll() is None
        if active is None:
          time.sleep(0.001)
      publish(rows['fresh'])
      active = None
      while active is None or active.carControl.actuators.accel != 2.0:
        active = messaging.recv_one_or_none(cc_socket)
        assert time.monotonic() < deadline and child.poll() is None
        if active is None:
          time.sleep(0.001)
      assert active.carControl.actuators.accel == 2.0
      (args.output / 'active-signal.bin').write_bytes(active.as_builder().to_bytes())
      child.send_signal(signal.SIGINT)
      assert child.wait(timeout=3) == 0
      checks['active_sigint'] = 0
    finally:
      if child.poll() is None:
        child.send_signal(signal.SIGTERM)
        child.wait(timeout=3)
  first = records[0]['carControl']['logMonoTime']
  last = records[-1]['carControl']['logMonoTime']
  elapsed = (last - first) / 1e9
  assert 2.0 <= elapsed <= 4.0, elapsed
  receipt = {'phases': len(matches), 'publications_per_service': len(records),
    'duration_seconds': elapsed, 'rate_hz': (len(records) - 1) / elapsed, 'startup_read_error_recovered': True,
    'exit_codes': checks, 'invalid_cli_cases': 3, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'command': [binary, '--frames', '240'], 'prefix': os.environ['OPENPILOT_PREFIX']}
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))
  shutil.rmtree(ipc_path)


if __name__ == '__main__':
  main()
