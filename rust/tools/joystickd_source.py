#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp"]
# ///
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import random
import struct
import sys
from types import SimpleNamespace

from pytest import MonkeyPatch

ROOT = Path(__file__).resolve().parents[2]


def load_binding(path: Path) -> None:
  spec = importlib.util.spec_from_file_location('openpilot.common.params_pyx', path)
  if spec is None or spec.loader is None:
    raise RuntimeError('source Params binding is unavailable')
  module = importlib.util.module_from_spec(spec)
  sys.modules[spec.name] = module
  spec.loader.exec_module(module)


class Finished(Exception):
  pass


def bits(value: float) -> int:
  return struct.unpack('<I', struct.pack('<f', value))[0]


def inputs() -> list:
  randomizer = random.Random(208)
  rows = []
  for frame in range(1, 601):
    axes = [randomizer.uniform(-2, 2), randomizer.uniform(-2, 2)]
    age = (0, 1, 19, 20, 21, 30)[frame % 6]
    rows.append({
      'enabled': frame % 13 != 0, 'active': frame % 7 != 0,
      'steer_fault_temporary': frame % 19 == 0, 'steer_fault_permanent': frame % 23 == 0,
      'override_longitudinal': frame % 17 == 0, 'cruise_enabled': frame % 3 == 0,
      'speed': (-2.0, 0.0, 0.5, 0.5001, 1.0, 5.0, 15.0, 35.0)[frame % 8],
      'steering_angle': randomizer.uniform(-300, 300), 'roll': randomizer.uniform(-0.15, 0.15),
      'angle_offset': randomizer.uniform(-2, 2), 'frame': frame + 30,
      'joystick_frame': 0 if frame % 29 == 0 else frame + 30 - age, 'axes': axes,
    })
  return rows


class SourceRun:
  def __init__(self, original, cp, rows: list, output: Path):
    self.original, self.cp, self.rows, self.output = original, cp, rows, output
    self.index, self.frame, self.recv_frame = -1, 0, {}
    self.values, self.expected, self.records, self.normalized = {}, [], [], []
    self.keep_times = 0

  def __getitem__(self, name):
    return self.values[name]

  def update(self, timeout):
    assert timeout == 0
    self.index += 1
    if self.index == len(self.rows):
      raise Finished()
    row = self.rows[self.index]
    messaging = self.original.messaging
    car = messaging.new_message('carState').carState
    car.vEgo, car.steeringAngleDeg = row['speed'], row['steering_angle']
    car.steerFaultTemporary, car.steerFaultPermanent = row['steer_fault_temporary'], row['steer_fault_permanent']
    car.cruiseState.enabled = row['cruise_enabled']
    drive = messaging.new_message('selfdriveState').selfdriveState
    drive.enabled, drive.active = row['enabled'], row['active']
    live = messaging.new_message('liveParameters').liveParameters
    live.roll, live.angleOffsetDeg = row['roll'], row['angle_offset']
    joystick = messaging.new_message('testJoystick').testJoystick
    joystick.axes = row['axes']
    events = messaging.new_message('onroadEvents', 1 if row['override_longitudinal'] else 0).onroadEvents
    if row['override_longitudinal']:
      events[0].overrideLongitudinal = True
    self.values = {'carState': car, 'selfdriveState': drive, 'liveParameters': live,
                   'testJoystick': joystick, 'onroadEvents': events}
    self.frame, self.recv_frame = row['frame'], {'testJoystick': row['joystick_frame']}
    self.normalized.append(row | {'speed': car.vEgo, 'steering_angle': car.steeringAngleDeg,
      'roll': live.roll, 'angle_offset': live.angleOffsetDeg, 'axes': list(joystick.axes)})
    self.expected.append({'control': None, 'curvature': None, 'error': None})

  def send(self, name, event):
    raw = event.to_bytes()
    (self.output / f'{len(self.records):04}-{name}.bin').write_bytes(raw)
    self.records.append({'service': name, 'valid': event.valid, 'data': getattr(event, name).to_dict()})
    row = self.expected[-1]
    match name:
      case 'carControl':
        cc = event.carControl
        row['control'] = {'enabled': cc.enabled, 'lat_active': cc.latActive, 'long_active': cc.longActive,
          'cancel': cc.cruiseControl.cancel, 'resume': cc.cruiseControl.resume,
          'lead_distance_bars': cc.hudControl.leadDistanceBars, 'long_state': str(cc.actuators.longControlState),
          'actuators': [bits(v) for v in (cc.actuators.accel, cc.actuators.torque,
                                         cc.actuators.steeringAngleDeg, cc.actuators.curvature)]}
      case 'controlsState':
        assert row['control'] is not None
        assert str(event.controlsState.lateralControlState.which()) == 'debugState'
        row['curvature'] = bits(event.controlsState.curvature)
      case _:
        raise AssertionError(name)
    assert event.valid

  def keep_time(self):
    self.keep_times += 1

  def run(self):
    def subscribe(names, *, frequency):
      assert names == ['carState', 'onroadEvents', 'liveParameters', 'selfdriveState', 'testJoystick']
      assert frequency == 100
      return self

    def publish(names):
      assert names == ['carControl', 'controlsState']
      return self

    def ratekeeper(rate, *, print_delay_threshold):
      assert rate == 100 and print_delay_threshold is None
      return self

    with MonkeyPatch.context() as patch:
      patch.setattr(self.original, 'Params', lambda: SimpleNamespace(get=lambda key, block: self.cp.to_bytes()))
      patch.setattr(self.original.messaging, 'SubMaster', subscribe)
      patch.setattr(self.original.messaging, 'PubMaster', publish)
      patch.setattr(self.original, 'Ratekeeper', ratekeeper)
      try:
        self.original.main()
      except Finished:
        assert self.keep_times == len(self.rows)
      except (IndexError, ZeroDivisionError) as error:
        self.expected[-1]['error'] = type(error).__name__
    (self.output / 'publications.json').write_text(json.dumps(self.records) + '\n')
    (self.output / 'CarParams.bin').write_bytes(self.cp.to_bytes())
    return self.normalized, self.expected


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  load_binding(args.binding.resolve())
  from openpilot.cereal import car
  from openpilot.tools.joystick import joystickd as original
  args.output.mkdir(parents=True, exist_ok=False)
  requests, expected = [], []
  common = {'mass': 1600.0, 'rotationalInertia': 2500.0, 'wheelbase': 2.5, 'centerToFront': 1.0,
    'steerRatioRear': 0.0, 'tireStiffnessFront': 100000.0, 'tireStiffnessRear': 100000.0, 'steerRatio': 16.0,
    'vEgoStopping': 0.5, 'openpilotLongitudinalControl': True, 'pcmCruise': True}
  rows = inputs()
  cases = [({}, rows), ({'pcmCruise': False}, rows), ({'openpilotLongitudinalControl': False}, rows)]
  for active, enabled, axes in ((True, True, []), (True, False, [0.0]), (False, False, [])):
    cases.append(({}, [rows[0] | {'active': active, 'enabled': enabled, 'axes': axes}]))
  cases.append(({'steerRatio': 0.0}, [rows[0] | {'active': False}]))
  cases.append(({'steerRatioRear': 1.0}, [rows[0] | {'active': True}]))
  for index, (changes, values) in enumerate(cases):
    cp = car.CarParams.new_message(**(common | changes))
    path = args.output / str(index)
    path.mkdir()
    normalized, result = SourceRun(original, cp, values, path).run()
    physical = {'mass': cp.mass, 'inertia': cp.rotationalInertia, 'wheelbase': cp.wheelbase,
      'center_front': cp.centerToFront, 'rear_ratio': cp.steerRatioRear,
      'stiffness_front': cp.tireStiffnessFront, 'stiffness_rear': cp.tireStiffnessRear, 'steer_ratio': cp.steerRatio}
    requests.append({'config': {'physical': physical, 'stopping_speed': cp.vEgoStopping,
      'openpilot_longitudinal': cp.openpilotLongitudinalControl, 'pcm_cruise': cp.pcmCruise}, 'inputs': normalized})
    expected.append(result)
  (args.output / 'input.json').write_text(json.dumps(requests, allow_nan=False) + '\n')
  (args.output / 'expected.json').write_text(json.dumps(expected, allow_nan=False) + '\n')
  sources = [Path(original.__file__), ROOT / 'opendbc_repo/opendbc/car/vehicle_model.py']
  receipt = {'cases': len(requests), 'steps': sum(map(len, expected)), 'source_sha256': {
    str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}}
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
