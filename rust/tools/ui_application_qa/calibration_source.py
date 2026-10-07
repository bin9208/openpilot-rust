from __future__ import annotations

import ast
from pathlib import Path
from types import SimpleNamespace
import math
import numpy as np
import pyray as rl
from openpilot.cereal import log
from openpilot.common.transformations.camera import DEVICE_CAMERAS, view_frame_from_device_frame
from openpilot.common.transformations.orientation import rot_from_euler
from ui_application_qa.qa_shapes import CalibrationStep, CalibrationResult


class Messages(dict):
  def __init__(self) -> None:
    super().__init__()
    self.seen: dict[str, bool] = {}
    self.updated: dict[str, bool] = {}
    self.valid: dict[str, bool] = {}
    self.recv_frame: dict[str, int] = {'liveCalibration': 0}

  def apply(self, packets: list[list[int]], index: int) -> None:
    self.updated = dict.fromkeys(['deviceState', 'roadCameraState', 'liveCalibration', 'carState'], False)
    for packet in packets:
      with log.Event.from_bytes(bytes(packet)) as event:
        name = event.which()
        self[name] = getattr(event, name).as_builder()
        self.seen[name] = True
        self.updated[name] = True
        self.valid[name] = event.valid
        self.recv_frame[name] = index + 1


class Model:
  def __init__(self) -> None:
    self.transform = np.eye(3)

  def set_transform(self, transform: np.ndarray) -> None:
    self.transform = transform


class Parameters:
  def __init__(self) -> None:
    self.position = ''

  def put(self, key: str, value: str) -> None:
    assert key == 'DevicePosition'
    self.position = value


def oracle(root: Path, big: bool, steps: list[CalibrationStep]) -> list[CalibrationResult]:
  source = root / ('openpilot/selfdrive/ui/onroad/augmented_road_view.py' if big else 'openpilot/selfdrive/ui/mici/onroad/augmented_road_view.py')
  body = ast.parse(source.read_text())
  methods = [node for node in body.body if isinstance(node, ast.ClassDef) and node.name == 'AugmentedRoadView'][0].body
  methods = [node for node in methods if isinstance(node, ast.FunctionDef) and node.name in ['_update_calibration', '_calc_frame_matrix']]
  assert len(methods) == 2
  ui = SimpleNamespace(sm=Messages())
  namespace = {
    'np': np,
    'rl': rl,
    'ui_state': ui,
    'DEVICE_CAMERAS': DEVICE_CAMERAS,
    'rot_from_euler': rot_from_euler,
    'view_frame_from_device_frame': view_frame_from_device_frame,
    'DEFAULT_DEVICE_CAMERA': DEVICE_CAMERAS['tici', 'ar0231'],
    'CALIBRATED': log.LiveCalibrationData.Status.calibrated,
    'WIDE_CAM': 2,
    'INF_POINT': np.array([1000.0, 0.0, 0.0]),
    'CAM_Y_OFFSET': 20,
    'math': math,
  }
  compiled = ast.ClassDef(name='OriginalCalibration', bases=[], keywords=[], body=methods, decorator_list=[])
  exec(compile(ast.fix_missing_locations(ast.Module(body=[compiled], type_ignores=[])), str(source), 'exec'), namespace)
  parameters = Parameters()
  instance = None
  results = []
  for index, step in enumerate(steps):
    if instance is None or step.get('reset', False):
      instance = namespace['OriginalCalibration']()
      instance.device_camera = None
      instance.view_from_calib = view_frame_from_device_frame.copy()
      instance.view_from_wide_calib = view_frame_from_device_frame.copy()
      instance._matrix_cache_key = (0, 0.0, 0.0, 0)
      instance._cached_matrix = None
      instance._last_device_position = ''
      instance.params = parameters
      instance.model_renderer = instance._model_renderer = Model()
    ui.sm.apply(step['messages'], index)
    instance.stream_type = step['stream']
    instance._content_rect = rl.Rectangle(*(step['rect'][key] for key in ['x', 'y', 'width', 'height']))
    instance._update_calibration()
    matrix = instance._calc_frame_matrix(instance._content_rect)
    results.append({'camera': matrix.tolist(), 'model': instance.model_renderer.transform.tolist(), 'position': parameters.position})
  return results
