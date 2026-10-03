"""Unchanged lagd/helper bodies and a fixed preimplementation numeric comparison contract."""

import ast
from collections import deque
from functools import cache, partial
import math
from pathlib import Path
import sys
from types import SimpleNamespace
from typing import Any

import capnp
import numpy as np
from openpilot.cereal import car, log
from openpilot.common.transformations.transformations import euler2rot_single, rot2euler_single

ROOT = Path(__file__).resolve().parents[2]


def source():
  scope = {
    '__name__': 'lagd_reference',
    'np': np,
    'log': log,
    'car': car,
    'capnp': capnp,
    'Any': Any,
    'cache': cache,
    'deque': deque,
    'partial': partial,
    'rot_from_euler': euler2rot_single,
    'euler_from_rot': rot2euler_single,
    'Params': object,
  }

  def new_message(service):
    value = log.Event.new_message(logMonoTime=123456789, valid=False)
    value.init(service)
    return value

  scope['messaging'] = SimpleNamespace(new_message=new_message)
  for name in ['helpers.py', 'lagd.py']:
    path = ROOT / 'openpilot/selfdrive/locationd' / name
    tree = ast.parse(path.read_text())
    tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
    exec(compile(tree, str(path), 'exec'), scope)
  return scope


def normalized(value):
  if isinstance(value, np.ndarray):
    return normalized(value.tolist())
  if isinstance(value, (tuple, list)):
    return [normalized(item) for item in value]
  if isinstance(value, dict):
    return {key: normalized(item) for key, item in value.items()}
  if isinstance(value, (float, np.floating)):
    if not math.isfinite(value):
      return 'nan' if math.isnan(value) else ('inf' if value > 0 else '-inf')
    return float(value)
  if isinstance(value, np.integer):
    return int(value)
  return value


def delay(scope, row):
  function = scope['LateralLagEstimator'].actuator_delay
  details = {}

  def trace(frame, event, arg):
    if frame.f_code == function.__code__ and event == 'return':
      values = frame.f_locals
      starts = values['starts'].tolist()
      details.update(
        peak=int(values['max_corr_index']), run=int(values['run_idx']) % len(starts), width=int(values['width']), starts=starts, ends=values['ends'].tolist()
      )
    return trace

  sys.settrace(trace)
  try:
    lag, corr, confidence = function(np.array(row['expected']), np.array(row['actual']), np.array(row['mask']), row['dt'], row['min'], row['max'])
  finally:
    sys.settrace(None)
  return normalized(dict(delay=lag, correlation=corr, confidence=confidence, **details))


def numeric(scope, row):
  match row['kind']:
    case 'padding':
      return [scope['fft_next_good_size'](value) for value in row['values']]
    case 'peak':
      return normalized(scope['parabolic_peak_interp'](np.array(row['values']), row['index']))
    case 'smooth':
      return normalized(scope['masked_symmetric_moving_average'](np.array(row['values']), np.array(row['mask']), row['k'], row['sigma']))
    case 'correlate':
      return normalized(scope['masked_normalized_cross_correlation'](np.array(row['expected']), np.array(row['actual']), np.array(row['mask']), row['n']))
    case 'delay':
      return delay(scope, row)
    case 'blocks':
      blocks = scope['BlockAverage'](row['count'], row['size'], row['valid'], row['initial'])
      output = []
      for value in [None] + row['updates']:
        if value is not None:
          blocks.update(value)
        output.append(
          {
            'values': blocks.values.flatten().tolist(),
            'block_idx': blocks.block_idx,
            'idx': blocks.idx,
            'valid_blocks': blocks.valid_blocks,
            'statistics': blocks.get(),
          }
        )
      return normalized(output)
    case 'pose':
      pose = scope['Pose'](
        *[
          scope['Measurement'](np.array(row['pose'][name]['xyz']), np.array(row['pose'][name]['std']))
          for name in ['orientation', 'velocity', 'acceleration', 'angular_velocity']
        ]
      )
      calibrator = scope['PoseCalibrator']()
      calibrator.feed_live_calib(SimpleNamespace(rpyCalib=row['rpy'], calStatus=int(row['valid'])))
      pose = calibrator.build_calibrated_pose(pose)
      return normalized(
        {
          'valid': calibrator.calib_valid,
          'rotation': calibrator.calib_from_device,
          'pose': [
            {'xyz': measurement.xyz, 'std': measurement.xyz_std} for measurement in [pose.orientation, pose.velocity, pose.acceleration, pose.angular_velocity]
          ],
        }
      )
  raise ValueError(row['kind'])


def compare(expected, actual, budget, path=''):
  if isinstance(expected, dict):
    assert isinstance(actual, dict) and expected.keys() == actual.keys(), (path, expected, actual)
    for key, value in expected.items():
      compare(value, actual[key], budget, path + '.' + key)
  elif isinstance(expected, list):
    assert isinstance(actual, list) and len(expected) == len(actual), (path, len(expected), actual)
    for index, value in enumerate(expected):
      compare(value, actual[index], budget, f'{path}[{index}]')
  elif isinstance(expected, float):
    assert isinstance(actual, (float, int)) and math.isclose(expected, actual, abs_tol=budget['absolute'], rel_tol=budget['relative']), (
      path,
      expected,
      actual,
      budget,
    )
  else:
    assert actual == expected, (path, expected, actual)
