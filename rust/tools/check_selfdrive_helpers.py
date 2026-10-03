import argparse
import ast
from enum import StrEnum, auto
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace as NS

import numpy as np

from openpilot.cereal import log
from openpilot.selfdrive.locationd.helpers import Measurement, Pose, PoseCalibrator
from openpilot.selfdrive.selfdrived.camera_config import get_camera_packets

ROOT = Path(__file__).resolve().parents[2]


def constant(path, name):
  tree = ast.parse((ROOT / path).read_text())
  node = next(node for node in tree.body if isinstance(node, ast.Assign)
              and any(isinstance(target, ast.Name) and target.id == name for target in node.targets))
  return ast.literal_eval(node.value)


def source_check():
  path = ROOT / 'openpilot/selfdrive/selfdrived/helpers.py'
  nodes = [node for node in ast.parse(path.read_text()).body if isinstance(node, (ast.Assign, ast.ClassDef))]
  namespace = {'StrEnum': StrEnum, 'auto': auto, 'math': math, 'car': NS(CarState=None), 'messaging': NS(SubMaster=None), 'Pose': Pose,
               'DT_CTRL': constant('openpilot/common/realtime.py', 'DT_CTRL'),
               'ACCELERATION_DUE_TO_GRAVITY': constant('opendbc_repo/opendbc/car/__init__.py', 'ACCELERATION_DUE_TO_GRAVITY'),
               'ISO_LATERAL_ACCEL': constant('opendbc_repo/opendbc/car/lateral.py', 'ISO_LATERAL_ACCEL'),
               'ACCEL_MIN': constant('opendbc_repo/opendbc/car/interfaces.py', 'ACCEL_MIN'),
               'ACCEL_MAX': constant('opendbc_repo/opendbc/car/interfaces.py', 'ACCEL_MAX')}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  return namespace['ExcessiveActuationCheck']


def pose(values=None):
  values = values or {}
  return {name: {'xyz': values.get(name, [0., 0., 0.]), 'xyz_std': [.1, .2, .3]}
          for name in ('orientation', 'velocity', 'acceleration', 'angular_velocity')}


def inputs():
  result = []
  for wide in (False, True):
    for disable in (-2147483648, -1, 0, 1, 2, 3, 2147483647):
      for simulation in (False, True):
        result.append({'reset': True, 'pose': pose(), 'cameras': {'wide': wide, 'disable_dm': disable, 'simulation': simulation}})
  rng = random.Random(168)
  for index in range(4000):
    values = {name: [rng.uniform(-3., 3.) for _ in range(3)] for name in pose()}
    value = pose(values)
    for measurement in value.values():
      measurement['xyz_std'] = [rng.uniform(-10., 10.) for _ in range(3)]
    result.append({'reset': index % 71 == 0, 'pose': value,
                   'calibration': {'rpy': [rng.uniform(-.7, .7) for _ in range(3)], 'calibrated': index % 2 == 0} if index % 3 == 0 else None})
  for angle in (-math.pi, -math.pi / 2, 0., math.pi / 2, math.pi):
    for axis in range(3):
      rpy = [0., 0., 0.]
      rpy[axis] = angle
      result.append({'reset': True, 'pose': pose({'orientation': rpy}), 'calibration': {'rpy': rpy, 'calibrated': True}})
  for acceleration in (-8., 5.):
    for adjacent in (math.nextafter(acceleration, -math.inf), acceleration, math.nextafter(acceleration, math.inf)):
      for error in (math.nextafter(2., -math.inf), 2., math.nextafter(2., math.inf), 0.):
        for active in (False, True):
          for frame in range(29):
            result.append({'reset': frame == 0, 'pose': pose({'acceleration': [adjacent, 0., 0.]}),
                           'actuation': {'longitudinal_active': active, 'lateral_active': False, 'steering_pressed': False,
                                         'ego_acceleration': adjacent + error, 'ego_speed': 0., 'roll': 0.}})
  for yaw in (-6., 6.):
    for adjacent in (math.nextafter(yaw, -math.inf), yaw, math.nextafter(yaw, math.inf)):
      for active in (False, True):
        for frame in range(150):
          result.append({'reset': frame == 0, 'pose': pose({'angular_velocity': [0., 0., adjacent]}),
                         'actuation': {'longitudinal_active': False, 'lateral_active': active, 'steering_pressed': frame == 140,
                                       'ego_acceleration': 0., 'ego_speed': 1., 'roll': 0.}})
  for frame in range(3000):
    acceleration = rng.choice([-10., -8., 0., 5., 6.])
    value = pose({'acceleration': [acceleration, 0., 0.], 'angular_velocity': [0., 0., rng.choice([-10., 0., 10.])]})
    result.append({'reset': frame == 0, 'pose': value,
                   'actuation': {'longitudinal_active': frame % 137 < 120, 'lateral_active': frame % 383 < 320,
                                 'steering_pressed': frame % 347 == 0, 'ego_acceleration': acceleration + rng.choice([0., 1., 2., -2.]),
                                 'ego_speed': rng.choice([0., .3, 10., 40.]), 'roll': rng.uniform(-.3, .3)}})
  for value in ('nan', 'inf', 'neg_inf'):
    for active in (False, True):
      result.append({'pose': pose(), 'nonfinite_roll': value,
                     'actuation': {'longitudinal_active': active, 'lateral_active': active, 'steering_pressed': False,
                                   'ego_acceleration': 0., 'ego_speed': 10., 'roll': 0.}})
  return result


def original(rows):
  check_type = source_check()
  calibrator, check = PoseCalibrator(), check_type()
  rpy = [0., 0., 0.]
  result = []
  for row in rows:
    if row.get('reset'):
      calibrator, check = PoseCalibrator(), check_type()
      rpy = [0., 0., 0.]
    if row.get('calibration') is not None:
      calibration = row['calibration']
      rpy = calibration['rpy']
      calibrator.feed_live_calib(NS(rpyCalib=rpy, calStatus=log.LiveCalibrationData.Status.calibrated if calibration['calibrated'] else
                                   log.LiveCalibrationData.Status.uncalibrated))
    measured = Pose(**{name: Measurement(np.array(value['xyz']), np.array(value['xyz_std'])) for name, value in row['pose'].items()})
    calibrated = calibrator.build_calibrated_pose(measured)
    excessive = None
    error = None
    if row.get('actuation') is not None:
      a = dict(row['actuation'])
      if row.get('nonfinite_roll'):
        a['roll'] = {'nan': math.nan, 'inf': math.inf, 'neg_inf': -math.inf}[row['nonfinite_roll']]
      sm = {'carControl': NS(longActive=a['longitudinal_active'], latActive=a['lateral_active']), 'liveParameters': NS(roll=a['roll'])}
      cs = NS(aEgo=a['ego_acceleration'], vEgo=a['ego_speed'], steeringPressed=a['steering_pressed'])
      try:
        excessive = check.update(sm, cs, calibrated)
      except ValueError as failure:
        assert math.isinf(a['roll']), failure
        error = str(failure)
    cameras = row.get('cameras')
    result.append({'calibrator': {'calibrated': calibrator.calib_valid, 'rpy': rpy, 'calib_from_device': calibrator.calib_from_device.tolist()},
                   'pose': {name: {field: value.tolist() for field, value in vars(measurement).items()} for name, measurement in vars(calibrated).items()},
                   'actuation': {'excessive_counter': check._excessive_counter, 'engaged_counter': check._engaged_counter},
                   'excessive': excessive,
                   'error': error,
                   'cameras': get_camera_packets(cameras['wide'], cameras['disable_dm'], cameras['simulation']) if cameras else None})
  return result


def compare(expected, actual, path=''):
  if isinstance(expected, dict):
    assert expected.keys() == actual.keys(), path
    for key in expected:
      compare(expected[key], actual[key], path + '/' + key)
  elif isinstance(expected, list):
    assert len(expected) == len(actual), path
    for index, (a, b) in enumerate(zip(expected, actual, strict=True)):
      compare(a, b, path + '/' + str(index))
  elif isinstance(expected, float):
    equivalent = actual is None if math.isnan(expected) else actual is not None and math.isclose(expected, actual, rel_tol=2e-12, abs_tol=2e-12)
    assert equivalent, (path, expected, actual)
  else:
    assert actual == expected, (path, expected, actual)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rows = inputs()
  expected = original(rows)
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  (args.output / 'source.json').write_text(json.dumps(expected))
  run = subprocess.run([str(args.native.resolve())], input=payload, text=True, capture_output=True, timeout=60)
  (args.output / 'native.jsonl').write_text(run.stdout)
  (args.output / 'native.stderr').write_text(run.stderr)
  assert run.returncode == 0, run.stderr
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  assert len(expected) == len(actual)
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    compare(source, native, str(index))
  paths = ['openpilot/selfdrive/locationd/helpers.py', 'openpilot/common/transformations/orientation.py',
           'openpilot/common/transformations/transformations.py', 'openpilot/selfdrive/selfdrived/helpers.py',
           'openpilot/selfdrive/selfdrived/camera_config.py', 'openpilot/common/realtime.py',
           'opendbc_repo/opendbc/car/interfaces.py', 'opendbc_repo/opendbc/car/lateral.py', 'opendbc_repo/opendbc/car/__init__.py']
  report = {'passed': True, 'steps': len(rows), 'numeric_atol_rtol': 2e-12,
            'source_hashes': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths},
            'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2))
  print(json.dumps({'passed': True, 'steps': len(rows)}))


if __name__ == '__main__':
  main()
