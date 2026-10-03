import ast
from collections import defaultdict
from enum import Enum
import importlib.util
from pathlib import Path
import sys
import types
from typing import Optional

import capnp
import numpy as np
from openpilot.cereal import log


def load(directory: Path):
  root = Path(__file__).resolve().parents[2]
  name = 'rednose.helpers.ekf_sym_pyx'
  extension = next(directory.glob('ekf_sym_pyx*.so'))
  spec = importlib.util.spec_from_file_location(name, extension)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  sys.modules[name] = module
  from openpilot.selfdrive.locationd.models.pose_kf import PoseKalman, States
  from openpilot.selfdrive.locationd.models.constants import ObservationKind
  from openpilot.common.transformations.orientation import rot_from_euler
  from openpilot.selfdrive.locationd.helpers import rotate_std

  tree = ast.parse((root / 'openpilot/cereal/messaging/__init__.py').read_text())
  new_message = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'new_message')
  clock = types.SimpleNamespace(monotonic=lambda: 1.23456789)
  messaging = {'Optional': Optional, 'capnp': capnp, 'log': log, 'time': clock}
  exec(compile(ast.Module(body=[new_message], type_ignores=[]), 'source-new-message', 'exec'), messaging)
  rows = []
  cloudlog = types.SimpleNamespace(**{level: lambda text, level=level: rows.append([level, text]) for level in ('warning', 'error')})
  namespace = {
    'np': np,
    'capnp': capnp,
    'Enum': Enum,
    'defaultdict': defaultdict,
    'log': log,
    'time': clock,
    'cloudlog': cloudlog,
    'PoseKalman': PoseKalman,
    'States': States,
    'ObservationKind': ObservationKind,
    'GENERATED_DIR': str(directory),
    'rot_from_euler': rot_from_euler,
    'rotate_std': rotate_std,
    'messaging': types.SimpleNamespace(new_message=messaging['new_message']),
  }
  tree = ast.parse((root / 'openpilot/selfdrive/locationd/locationd.py').read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom, ast.If))]
  exec(compile(tree, 'source-locationd', 'exec'), namespace)
  return namespace, rows
