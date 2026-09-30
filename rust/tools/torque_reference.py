"""Original source oracle and predeclared numeric comparison boundaries."""

from __future__ import annotations
import ast
import logging
from pathlib import Path
from types import ModuleType
import numpy as np
from calibration_reference import original as calibration_original, Parameters as CalibrationParameters
from openpilot.cereal import car, log
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.selfdrive.locationd.helpers import PointBuckets, ParameterEstimator, PoseCalibrator, Pose

ROOT = Path(__file__).resolve().parents[2]


class Parameters:
  def __init__(self, previous: bytes | None, saved: bytes | None):
    self.values = {"CarParamsPrevRoute": previous, "LiveTorqueParameters": saved}
    self.removed = False
    self.writes: list[bytes] = []

  def get(self, key: str) -> bytes | None:
    return self.values[key]

  def remove(self, key: str) -> None:
    assert key == "LiveTorqueParameters"
    self.removed = True

  def put_nonblocking(self, key: str, value: bytes) -> None:
    assert key == "LiveTorqueParameters"
    self.writes.append(value)


def original(params: Parameters) -> ModuleType:
  assert np.__version__ == "2.5.3", "source oracle must use repository-locked NumPy2.5.3"
  path = ROOT / "openpilot/selfdrive/locationd/torqued.py"
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [
    node
    for node in tree.body
    if not (isinstance(node, ast.ImportFrom) and (node.module or "").startswith("openpilot"))
    and not (isinstance(node, ast.Import) and any(alias.name.startswith("openpilot") for alias in node.names))
    and not (isinstance(node, ast.If) and isinstance(node.test, ast.Compare) and isinstance(node.test.left, ast.Name) and node.test.left.id == "__name__")
  ]
  module = ModuleType("torque_source")
  module.__dict__.update(
    car=car,
    log=log,
    ACCELERATION_DUE_TO_GRAVITY=9.81,
    DT_MDL=0.05,
    Params=lambda: params,
    FirstOrderFilter=FirstOrderFilter,
    cloudlog=logging.getLogger("torque-source"),
    PointBuckets=PointBuckets,
    ParameterEstimator=ParameterEstimator,
    PoseCalibrator=PoseCalibrator,
    Pose=Pose,
    messaging=calibration_original(False, CalibrationParameters()).messaging,
  )
  exec(compile(tree, str(path), "exec"), module.__dict__)
  main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
  body = next(node.body for node in main.body if isinstance(node, ast.While))
  module.loop_body = compile(ast.Module(body=body, type_ignores=[]), str(path), "exec")
  return module


def close(actual: float, expected: float, tolerance: float = 2e-10, wire: bool = False) -> bool:
  if np.isnan(expected):
    return bool(np.isnan(actual))
  if np.isinf(expected):
    return actual == expected
  if not np.isfinite(actual):
    return False
  limit = tolerance + tolerance * abs(expected)
  if wire:
    with np.errstate(over="ignore"):
      adjacent = [np.nextafter(np.float32(expected), side, dtype=np.float32) for side in (-np.inf, np.inf)]
    limit = max([2e-10, *(abs(float(value) - expected) for value in adjacent if np.isfinite(value))])
  return abs(actual - expected) <= limit


def compare(actual, expected) -> int:
  if isinstance(expected, dict):
    assert actual.keys() == expected.keys(), (actual.keys(), expected.keys())
    return sum(compare(actual[key], value) for key, value in expected.items())
  if isinstance(expected, list):
    assert len(actual) == len(expected)
    return sum(compare(a, b) for a, b in zip(actual, expected, strict=True))
  if isinstance(expected, float):
    assert close(actual, expected, wire=True), (actual, expected)
  else:
    assert actual == expected, (actual, expected)
  return 1
