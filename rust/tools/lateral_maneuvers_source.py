#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp"]
# ///
from __future__ import annotations

import argparse
import hashlib
import inspect
import json
from pathlib import Path
import struct
from types import SimpleNamespace

from pytest import MonkeyPatch
from joystickd_source import load_binding


class Finished(Exception):
  pass


def bits(value: float) -> int:
  return struct.unpack('=Q', struct.pack('=d', value))[0]


class SourceRun:
  def __init__(self, original, cp):
    self.original, self.cp = original, cp
    self.values, self.inputs, self.expected = {}, [], []
    self.steps = [0] * len(original.MANEUVERS)
    self.resets = [set() for _ in original.MANEUVERS]
    self.completed = 0

  def __getitem__(self, name):
    return self.values[name]

  def update(self):
    frame = inspect.currentframe().f_back
    owner = frame.f_locals['maneuver']
    if owner is None:
      owner = next((owner for owner in self.original.MANEUVERS if not owner.finished), None)
    selected = next((index for index, candidate in enumerate(self.original.MANEUVERS) if candidate is owner), None)
    if selected is None:
      self.completed += 1
      if self.completed > 4:
        raise Finished()
    step = self.steps[selected] if selected is not None else self.completed
    if selected is not None:
      self.steps[selected] += 1
    speed = owner.initial_speed if owner else 0.0
    active, override, valid = True, False, True
    curvature = (.001 + owner._repeated * .0002) * (-1 if selected % 2 else 1) if owner else 0.0
    orientation = [0.0, 0.0, 0.0]
    if owner and step < 12:
      speed += [0.0, 1.0, -.8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0][step]
      active = step != 0
      if step in (3, 4):
        curvature = .0025
      if step == 5:
        orientation = [.09, 0.0, 0.0]
      if step in (6, 7, 8, 9):
        orientation = [.09] * (step - 6)
      if step == 10:
        active = False
    if owner and owner.active and not frame.f_locals['complete_cnt']:
      if owner._action_frames == 3 and 'steering' not in self.resets[selected]:
        override = True
        self.resets[selected].add('steering')
      if owner._action_frames == 6 and 'speed' not in self.resets[selected]:
        speed += .8
        self.resets[selected].add('speed')
      if owner._action_frames == 8:
        active, valid = False, False
      if owner._action_frames == 9:
        curvature, orientation = .004, [.2, 0.0, 0.0]
    if owner and frame.f_locals['complete_cnt'] > 0:
      override, active = True, False
      if owner.finished:
        speed = -.5
      valid = frame.f_locals['complete_cnt'] != 10
    if not self.inputs:
      speed, active, override, curvature, orientation, valid = 0.0, False, False, 0.0, [], False
    messaging = self.original.messaging
    car = messaging.new_message('carState').carState
    car.vEgo, car.steeringPressed = speed, override
    control = messaging.new_message('carControl').carControl
    control.latActive, control.orientationNED = active, orientation
    controls = messaging.new_message('controlsState').controlsState
    controls.desiredCurvature = curvature
    self.values = {'carState': car, 'carControl': control, 'controlsState': controls}
    self.inputs.append({'speed': car.vEgo, 'active': active, 'steering_pressed': override,
      'curvature': controls.desiredCurvature, 'orientation': list(control.orientationNED), 'valid': valid})
    self.row = {'messages': [], 'acceleration_bits': None, 'baseline_bits': None,
      'selected': selected, 'state': None, 'complete_remaining': None, 'display_holdoff': None}
    self.expected.append(self.row)

  def send(self, name, event):
    self.row['messages'].append({'service': name, 'valid': event.valid, 'data': getattr(event, name).to_dict()})
    if name == 'lateralManeuverPlan':
      frame = inspect.currentframe().f_back
      owner = frame.f_locals['maneuver']
      self.row['acceleration_bits'] = bits(frame.f_locals['accel'])
      self.row['baseline_bits'] = bits(owner._baseline_curvature if owner else 0.0)
      self.row['complete_remaining'] = frame.f_locals['complete_cnt']
      self.row['display_holdoff'] = frame.f_locals['display_holdoff']
      if owner:
        self.row['state'] = {name: getattr(owner, '_' + name) for name in
          ('active', 'finished', 'run_completed', 'action_index', 'action_frames', 'repeated')}
        self.row['state']['ready_count'] = owner._ready_cnt

  def run(self):
    def subscribe(names, *, poll):
      assert names == ['carState', 'carControl', 'controlsState', 'selfdriveState', 'modelV2'] and poll == 'modelV2'
      return self

    def publish(names):
      assert names == ['lateralManeuverPlan', 'alertDebug']
      return self

    with MonkeyPatch.context() as patch:
      patch.setattr(self.original, 'Params', lambda: SimpleNamespace(get=lambda key, block: self.cp.to_bytes()))
      patch.setattr(self.original.messaging, 'SubMaster', subscribe)
      patch.setattr(self.original.messaging, 'PubMaster', publish)
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
  from openpilot.tools.lateral_maneuvers import lateral_maneuversd as original
  args.output.mkdir(parents=True, exist_ok=False)
  cp = car.CarParams.new_message()
  run = SourceRun(original, cp)
  run.run()
  (args.output / 'input.json').write_text(json.dumps({'inputs': run.inputs}) + '\n')
  (args.output / 'expected.json').write_text(json.dumps(run.expected) + '\n')
  cp.clear_write_flag()
  (args.output / 'CarParams.bin').write_bytes(cp.to_bytes())
  receipt = {'steps': len(run.inputs), 'presets': len(original.MANEUVERS),
    'completed': sum(owner.finished for owner in original.MANEUVERS),
    'source_sha256': hashlib.sha256(Path(original.__file__).read_bytes()).hexdigest()}
  assert receipt['completed'] == 6
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()

