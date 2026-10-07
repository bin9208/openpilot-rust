#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
from types import SimpleNamespace

from pytest import MonkeyPatch
from joystickd_source import load_binding


class Finished(Exception):
  pass


class SourceRun:
  def __init__(self, original, cp):
    self.original, self.cp = original, cp
    self.values, self.inputs, self.expected = {}, [], []
    self.steps = [0] * len(original.MANEUVERS)
    self.held_repeat = [False] * len(original.MANEUVERS)
    self.completed = 0
    self.row = None

  def __getitem__(self, name):
    return self.values[name]

  def update(self):
    selected = next((index for index, owner in enumerate(self.original.MANEUVERS) if not owner.finished), None)
    if selected is None:
      self.completed += 1
      if self.completed > 5:
        raise Finished()
    frame = self.steps[selected] if selected is not None else self.completed
    owner = self.original.MANEUVERS[selected] if selected is not None else None
    speed = owner.initial_speed if owner else (0.5 if frame == 1 else 0.49)
    active, standstill, cruise, valid = True, speed < .01, False, True
    if selected is not None:
      self.steps[selected] += 1
      if frame < 4:
        speed = [-2.0, speed + 3, speed - 3, speed][frame]
        active = frame == 3
      if frame in (5, 40, 200):
        active = False
      if frame in (6, 41, 201):
        cruise = True
      if frame == 7:
        standstill = False
      if frame in (10, 250):
        valid = False
      if owner._run_completed and owner._repeated == 1 and not self.held_repeat[selected]:
        active = False
        self.held_repeat[selected] = True
      if owner.active and frame == 180:
        speed = -.5
    if not self.inputs:
      speed, active, standstill, cruise, valid = 0.0, False, False, False, False
    messaging = self.original.messaging
    car = messaging.new_message('carState').carState
    car.vEgo, car.standstill, car.cruiseState.standstill = speed, standstill, cruise
    control = messaging.new_message('carControl').carControl
    control.longActive = active
    self.values = {'carState': car, 'carControl': control}
    self.accel = 0.0
    self.inputs.append({'speed': car.vEgo, 'active': active, 'standstill': standstill, 'cruise_standstill': cruise, 'valid': valid})
    self.row = {'messages': [], 'acceleration_bits': None, 'selected': selected, 'state': None}
    self.expected.append(self.row)

  def all_checks(self):
    return self.inputs[-1]['valid']

  def send(self, name, event):
    self.row['messages'].append({'service': name, 'valid': event.valid, 'data': getattr(event, name).to_dict()})
    if name == 'driverAssistance' and self.row['selected'] is not None:
      owner = self.original.MANEUVERS[self.row['selected']]
      self.row['state'] = {name: getattr(owner, '_' + name) for name in
        ('active', 'finished', 'run_completed', 'action_index', 'action_frames', 'repeated')}
      self.row['state']['ready_count'] = owner._ready_cnt
    if name == 'longitudinalPlan':
      self.row['acceleration_bits'] = struct.unpack('=Q', struct.pack('=d', self.accel))[0]

  def run(self):
    def subscribe(names, *, poll):
      assert names == ['carState', 'carControl', 'controlsState', 'selfdriveState', 'modelV2'] and poll == 'modelV2'
      return self

    def publish(names):
      assert names == ['longitudinalPlan', 'driverAssistance', 'alertDebug']
      return self

    originals = [owner.get_accel for owner in self.original.MANEUVERS]
    with MonkeyPatch.context() as patch:
      patch.setattr(self.original, 'Params', lambda: SimpleNamespace(get=lambda key, block: self.cp.to_bytes()))
      patch.setattr(self.original.messaging, 'SubMaster', subscribe)
      patch.setattr(self.original.messaging, 'PubMaster', publish)
      for owner, method in zip(self.original.MANEUVERS, originals, strict=True):
        def observed(*args, method=method):
          self.accel = method(*args)
          return self.accel
        patch.setattr(owner, 'get_accel', observed)
      try:
        self.original.main()
      except Finished:
        return


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  load_binding(args.binding.resolve())
  from openpilot.cereal import car
  from openpilot.tools.longitudinal_maneuvers import maneuversd as original
  args.output.mkdir(parents=True, exist_ok=False)
  cp = car.CarParams.new_message(vEgoStopping=.5)
  run = SourceRun(original, cp)
  run.run()
  (args.output / 'input.json').write_text(json.dumps({'stopping_speed': cp.vEgoStopping, 'inputs': run.inputs}) + '\n')
  (args.output / 'expected.json').write_text(json.dumps(run.expected) + '\n')
  cp.clear_write_flag()
  (args.output / 'CarParams.bin').write_bytes(cp.to_bytes())
  receipt = {'steps': len(run.inputs), 'presets': len(original.MANEUVERS), 'completed': sum(owner.finished for owner in original.MANEUVERS),
    'source_sha256': hashlib.sha256(Path(original.__file__).read_bytes()).hexdigest()}
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
