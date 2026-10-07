#!/usr/bin/env python3
import ast
import argparse
import dataclasses
import importlib
import json
from pathlib import Path
import sys
import subprocess
import types

import numpy as np


def source_modules():
  root = Path(__file__).resolve().parents[2]
  realtime = types.ModuleType('openpilot.common.realtime')
  tree = ast.parse((root / 'openpilot/common/realtime.py').read_text())
  for node in tree.body:
    if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name) and node.targets[0].id.startswith('DT_'):
      setattr(realtime, node.targets[0].id, ast.literal_eval(node.value))
  sys.modules[realtime.__name__] = realtime
  return tuple(importlib.import_module('openpilot.selfdrive.controls.lib.' + name) for name in ('ldw', 'cruise_coasting', 'longitudinal_cutout'))


@dataclasses.dataclass(frozen=True)
class Prediction:
  left_probability: float = 0.2
  right_probability: float = 0.3
  left_visibility: float = 0.6
  right_visibility: float = 0.7
  left_y: float = -1.0
  right_y: float = 1.0


@dataclasses.dataclass(frozen=True)
class DepartureInput:
  frame: int = 500
  speed: float = 20.0
  left_blinker: bool = False
  right_blinker: bool = False
  lateral_active: bool = False
  prediction: Prediction | None = Prediction()


def departure(source):
  from openpilot.cereal import car, log

  inputs = [DepartureInput(frame=frame) for frame in (0, 1, 499, 500, 501)]
  inputs.extend(
    (DepartureInput(frame=700, right_blinker=True), DepartureInput(frame=1199), DepartureInput(frame=1200), DepartureInput(frame=1201, prediction=None))
  )
  inputs.extend(
    DepartureInput(frame=1500 + index, **fields)
    for index, fields in enumerate(
      [
        {'speed': 31 * source.CV.MPH_TO_MS},
        {'speed': 31 * source.CV.MPH_TO_MS + 1e-12},
        {'lateral_active': True},
        {'prediction': Prediction(left_visibility=0.5)},
        {'prediction': Prediction(right_visibility=0.5)},
        {'prediction': Prediction(left_probability=0.1, right_probability=0.1)},
        {'prediction': Prediction(left_y=-(1.08 + 0.04), right_y=1.08 - 0.04)},
        {'prediction': Prediction(left_y=-(1.08 + 0.04) + 1e-12, right_y=1.08 - 0.04 - 1e-12)},
      ]
    )
  )
  owner = source.LaneDepartureWarning()
  frames = []
  for item in inputs:
    model = log.ModelDataV2.new_message()
    state = car.CarState.new_message(vEgo=item.speed, leftBlinker=item.left_blinker, rightBlinker=item.right_blinker)
    control = car.CarControl.new_message(latActive=item.lateral_active)
    if item.prediction is not None:
      prediction = item.prediction
      model.meta.desirePrediction = [0.0, 0.0, 0.0, prediction.left_probability, prediction.right_probability, 0.0, 0.0, 0.0]
      model.laneLineProbs = [0.0, prediction.left_visibility, prediction.right_visibility, 0.0]
      model.init('laneLines', 4)
      model.laneLines[1].y = [prediction.left_y]
      model.laneLines[2].y = [prediction.right_y]
      item = dataclasses.replace(
        item,
        speed=float(state.vEgo),
        prediction=Prediction(
          float(model.meta.desirePrediction[3]),
          float(model.meta.desirePrediction[4]),
          float(model.laneLineProbs[1]),
          float(model.laneLineProbs[2]),
          float(model.laneLines[1].y[0]),
          float(model.laneLines[2].y[0]),
        ),
      )
    owner.update(item.frame, model.as_reader(), state.as_reader(), control.as_reader())
    frames.append({'input': dataclasses.asdict(item), 'expected': {'left': owner.left, 'right': owner.right}})
  return frames


def coasting(source):
  owner = source.CruiseCoastingPlan()
  frames = []
  base = {'enabled': True, 'percent': 5.0, 'set_speed': 21.0, 'target': 20.0, 'external_limit': 25.0, 'dt': 0.05}
  for fields in (
    {},
    {'target': 20.01},
    {'target': 20.03},
    {'set_speed': 21.002},
    {'external_limit': 21.0},
    {},
    {'enabled': False},
    {},
    {'percent': 0.9},
    {'percent': 10.9},
    {'percent': -1.0},
    {'dt': 0.0},
    {'target': 10.0 / 3.6},
    {'percent': 2.9},
  ):
    frame = base | fields
    for _ in range(23):
      target = owner.update(**frame)
      frames.append({'input': frame, 'expected': [target, owner.stable_time]})
  return frames


def lead_tau():
  from openpilot.selfdrive.carrot.radar_motion.lead_dynamics import LeadAccelTau

  owner = LeadAccelTau()
  rows = [
    (a, j, t, measured)
    for a, j, t, measured in [
      (-2.0, -4.0, 1.0, True),
      (-2.0, -4.0, 1.05, True),
      (-2.0, -4.0, 1.05, True),
      (-2.0, -4.0, 1.1, True),
      (1.0, 1.0, 1.15, True),
      (0.49, 0.49, 1.2, True),
      (-2.0, -4.0, 1.3, True),
      (-2.0, -4.0, 1.25, True),
      (-2.0, -4.0, 1.3, True),
      (-2.0, -4.0, 1.35, False),
      (-2.0, -4.0, 1.4, True),
      (-2.0, -4.0, 1.45, True),
      (-2.0, -4.0, 1.7, True),
      (-2.0, -4.0, 1.75, True),
      (0.0, 1.0, 1.8, True),
    ]
  ]
  frames = []
  for a, j, t, measured in rows:
    frame = {'acceleration': a, 'jerk': j, 'time': t, 'measured': measured}
    frames.append({'input': frame, 'expected': owner.update(a, j, t, measured=measured)})
  return frames


def geometry():
  path = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/controls/lib/lateral_planner.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ('smooth_moving_avg', 'yaw_from_path_no_scipy')]
  if len(tree.body) != 2:
    raise RuntimeError('source lateral geometry functions changed')
  namespace = {'np': np}
  exec(compile(tree, str(path), 'exec'), namespace)
  frames = []
  index = np.arange(33, dtype=float)
  for speed in (1.0, 5.0, 6.0, 6.001, 20.0, 35.0):
    for shape in range(5):
      x = index * (0.2 + 0.02 * shape) + index**2 * 0.003
      y = (index * 0.01) if shape == 0 else np.sin(index * 0.03 * shape) * (shape * 0.3)
      points = np.column_stack((x, y, np.zeros(33)))
      speeds = np.full(33, speed)
      yaw, rate = namespace['yaw_from_path_no_scipy'](points, speeds)
      frames.append({'input': {'path': points.tolist(), 'speeds': speeds.tolist()}, 'expected': [yaw.astype(float).tolist(), rate.astype(float).tolist()]})
  return frames


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--trace', type=Path)
  args = parser.parse_args()
  ldw, coast, _cutout = source_modules()
  cases = {'departure': departure(ldw), 'coasting': coasting(coast), 'lead_tau': lead_tau(), 'geometry': geometry()}
  request = {name: [frame['input'] for frame in frames] for name, frames in cases.items()}
  expected = {name: [frame['expected'] for frame in frames] for name, frames in cases.items()}
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / 'input.json').write_text(json.dumps(request, indent=2) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
  if args.trace is not None:
    child = subprocess.run([str(args.trace.resolve())], input=json.dumps(request), capture_output=True, text=True, check=False)
    (args.evidence / 'native.json').write_text(child.stdout)
    (args.evidence / 'native.stderr').write_text(child.stderr)
    child.check_returncode()
    actual = json.loads(child.stdout)
    if actual != expected:
      raise AssertionError('planner policy differs from the unchanged source; inspect evidence')
    print(json.dumps({'exact_frames': {name: len(frames) for name, frames in cases.items()}}))


if __name__ == '__main__':
  main()
