import ast
import importlib.util
import json
import math
from pathlib import Path
import sys
import types
from typing import Optional

import capnp
import numpy as np
from openpilot.cereal import car, log


def load(directory):
  root = Path(__file__).resolve().parents[2]
  name = 'rednose.helpers.ekf_sym_pyx'
  spec = importlib.util.spec_from_file_location(name, next(directory.glob('ekf_sym_pyx*.so')))
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  sys.modules[name] = module
  logs = []
  cloudlog = types.SimpleNamespace(**{level: lambda message, level=level: logs.append([level, message]) for level in ('info', 'warning', 'error')})
  swaglog = types.ModuleType('openpilot.common.swaglog')
  swaglog.cloudlog = cloudlog
  sys.modules[swaglog.__name__] = swaglog
  from openpilot.selfdrive.locationd.models.car_kf import CarKalman, States, ObservationKind
  from openpilot.selfdrive.locationd.helpers import PoseCalibrator, Pose

  tree = ast.parse((root / 'openpilot/cereal/messaging/__init__.py').read_text())
  function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'new_message')
  clock = types.SimpleNamespace(monotonic=lambda: 1.23456789)
  scope = {'Optional': Optional, 'capnp': capnp, 'log': log, 'time': clock}
  exec(compile(ast.Module(body=[function], type_ignores=[]), 'source-new-message', 'exec'), scope)
  namespace = {'np': np, 'math': math, 'json': json, 'capnp': capnp, 'time': clock, 'car': car, 'log': log,
               'messaging': types.SimpleNamespace(new_message=scope['new_message']), 'cloudlog': cloudlog,
               'CarKalman': CarKalman, 'States': States, 'ObservationKind': ObservationKind,
               'GENERATED_DIR': str(directory), 'PoseCalibrator': PoseCalibrator, 'Pose': Pose, 'DT_MDL': .05, 'Params': None}
  tree = ast.parse((root / 'openpilot/selfdrive/locationd/paramsd.py').read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
  exec(compile(tree, 'source-paramsd', 'exec'), namespace)
  return namespace, logs
