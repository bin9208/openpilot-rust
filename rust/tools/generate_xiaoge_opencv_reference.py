#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "opencv-python-headless==4.13.0.92"]
# ///
# Run with retained reference/venv/bin/python, PYTHONPATH=repository root.
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import shutil
from typing import Literal, TypeAlias, TypedDict
import cv2
import numpy as np
from numpy.typing import NDArray
from openpilot.selfdrive.carrot.xiaoge.lane_inference import LaneInference, prepare_lane_image
from openpilot.selfdrive.carrot.xiaoge.v_asm_inference import VASMInference

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']

class Request(TypedDict, total=False):
  op: Literal['resize', 'bgr_gray', 'nv12_rgb', 'polygon_mask', 'bounds', 'mask_image', 'dnn']
  input: str
  mask: str
  width: int
  height: int
  format: str
  output_width: int
  output_height: int
  points: list[list[int]]
  model: str
  shape: list[int]
  names: list[str]

class Case(TypedDict):
  name: str
  request: Request
  expected: dict[str, Json]

@dataclass(frozen=True, slots=True)
class ModelEvaluation:
  model: Path
  blob: NDArray[np.float32]
  outputs: tuple[NDArray[np.float32], ...]
  names: list[str]
  available_names: tuple[str, ...]

class VasmCapture:
  """Accumulate the actual original VASM blob and real OpenCV model output."""
  def __init__(self, net: cv2.dnn.Net) -> None:
    self.net = net
    self.blob: NDArray[np.float32] | None = None
    self.output: NDArray[np.float32] | None = None

  def setInput(self, blob: NDArray[np.float32]) -> None:
    self.blob = blob.copy()
    self.net.setInput(blob)

  def forward(self) -> NDArray[np.float32]:
    self.output = self.net.forward()
    return self.output

class MissingInference(RuntimeError):
  def __init__(self, side: str) -> None:
    self.side = side
    super().__init__(f'original VASM did not execute actual model for {side}')

class Oracle:
  """Accumulate named binary fixture files and typed reference requests."""
  def __init__(self, output: Path) -> None:
    self.output = output
    self.cases: list[Case] = []

  def data(self, name: str, value: NDArray[np.uint8] | NDArray[np.float32]) -> str:
    path = self.output / (name + '.bin')
    path.write_bytes(value.tobytes(order='C'))
    return str(path)

  def image(self, name: str, request: Request, value: NDArray[np.uint8]) -> None:
    self.cases.append({'name': name, 'request': request, 'expected': {'kind': 'image',
      'shape': list(value.shape), 'file': self.data(name + '-expected', value)}})

  def tensor(self, name: str, evaluation: ModelEvaluation) -> None:
    self.cases.append({'name': name, 'request': {'op': 'dnn', 'model': str(evaluation.model),
      'input': self.data(name + '-blob', evaluation.blob), 'shape': list(evaluation.blob.shape), 'names': evaluation.names},
      'expected': {'kind': 'dnn', 'available_names': list(evaluation.available_names),
        'outputs': [{'shape': list(value.shape), 'file': self.data(f'{name}-output-{i}', value)}
        for i, value in enumerate(evaluation.outputs)]}})


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--models', type=Path, required=True)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.models = args.models.resolve()
  assert cv2.__version__ == '4.13.0'
  assert np.__version__ == '2.5.3'
  assert shutil.disk_usage(args.output.parent).free >= 25 * 1024**3 + 256 * 1024**2
  args.output.mkdir()
  cv2.setNumThreads(2)
  (args.output / 'wheel-build-information.txt').write_text(cv2.getBuildInformation())
  oracle = Oracle(args.output)
  rng = np.random.default_rng(200)
  for index, (height, width, channels, out_height, out_width) in enumerate([
    (1, 1, 1, 3, 5), (7, 9, 1, 416, 416), (24, 32, 3, 256, 352), (17, 19, 3, 7, 9), (49, 65, 1, 11, 13)]):
    image = rng.integers(0, 256, (height, width, channels), dtype=np.uint8)
    if channels == 1:
      image = image[:, :, 0]
    oracle.image(f'resize-{index}', {'op': 'resize', 'input': oracle.data(f'resize-{index}-input', image),
      'width': width, 'height': height, 'format': 'gray' if channels == 1 else 'rgb',
      'output_width': out_width, 'output_height': out_height}, cv2.resize(image, (out_width, out_height), interpolation=cv2.INTER_LINEAR))
  bgr = rng.integers(0, 256, (7, 11, 3), dtype=np.uint8)
  oracle.image('bgr-gray', {'op': 'bgr_gray', 'input': oracle.data('bgr-gray-input', bgr), 'width': 11, 'height': 7},
    cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY))
  for index, (width, height) in enumerate([(6, 4), (80, 64)]):
    nv12 = rng.integers(0, 256, (height * 3 // 2, width), dtype=np.uint8)
    rgb = cv2.cvtColor(nv12, cv2.COLOR_YUV2RGB_NV12)
    oracle.image(f'nv12-{index}', {'op': 'nv12_rgb', 'input': oracle.data(f'nv12-{index}-input', nv12), 'width': width, 'height': height}, rgb)
  polygons = [[[1, 1], [12, 3], [7, 10]], [[-3, -2], [18, 0], [15, 14], [-1, 11]],
    [[0, 0], [15, 12], [0, 12], [15, 0]], [[7, 3], [7, 3], [7, 3]]]
  for index, vertices in enumerate(polygons):
    points = np.array(vertices, dtype=np.int32)
    mask = np.zeros((13, 16), dtype=np.uint8)
    cv2.fillPoly(mask, [points], 255)
    oracle.image(f'polygon-{index}', {'op': 'polygon_mask', 'width': 16, 'height': 13, 'points': vertices}, mask)
    x, y, width, height = cv2.boundingRect(points)
    oracle.cases.append({'name': f'bounds-{index}', 'request': {'op': 'bounds', 'points': vertices},
      'expected': {'kind': 'bounds', 'x': x, 'y': y, 'width': width, 'height': height}})
  rgb = rng.integers(0, 256, (13, 16, 3), dtype=np.uint8)
  mask = rng.choice(np.array([0, 1, 2, 255], dtype=np.uint8), (13, 16))
  oracle.image('mask-values', {'op': 'mask_image', 'input': oracle.data('mask-rgb', rgb), 'mask': oracle.data('mask-values', mask),
    'width': 16, 'height': 13, 'format': 'rgb'}, cv2.bitwise_and(rgb, rgb, mask=mask))
  lane = LaneInference(args.models / 'lane.onnx')
  assert lane.load(), lane.error
  for index, (height, width, color) in enumerate([(48, 64, False), (720, 1280, False), (37, 49, True)]):
    source = rng.integers(0, 256, (height, width, 3) if color else (height + height // 2, width + 16), dtype=np.uint8)
    gray = prepare_lane_image(source, width, height)
    oracle.data(f'lane-{index}-original-gray', gray)
    blob = lane.preprocess_image(source, width, height)
    lane.net.setInput(blob)
    names = list(lane.net.getUnconnectedOutLayersNames())
    outputs = tuple(lane.net.forward(names))
    oracle.tensor(f'lane-{index}', ModelEvaluation(lane.model_path, blob, outputs, names, tuple(names)))
    (args.output / f'lane-{index}-original-result.json').write_text(json.dumps(lane.infer(source, width, height)) + '\n')
  vasm = VASMInference(args.models / 'v_asm_model.onnx')
  assert vasm.load(), vasm.error
  vasm.load_config({'width': 64, 'height': 48, 'poly_left': [[2, 3], [58, 0], [63, 47], [0, 45]],
    'poly_right': [[30, -3], [70, 10], [56, 43], [26, 38]]})
  native = vasm.net
  for side in ('left', 'right'):
    capture = VasmCapture(native)
    vasm.net = capture
    width, height = 80, 64
    nv12 = rng.integers(0, 256, (height * 3 // 2, width), dtype=np.uint8)
    vasm._prepare_geometry(height, width)
    confidence = vasm._confidence(nv12, height, side)
    if capture.blob is None or capture.output is None:
      raise MissingInference(side)
    oracle.tensor('vasm-' + side, ModelEvaluation(vasm.model_path, capture.blob, (capture.output,), [], tuple(native.getUnconnectedOutLayersNames())))
    (args.output / ('vasm-' + side + '-original-geometry.json')).write_text(json.dumps({'bbox': vasm.bboxes[side], 'confidence': confidence}) + '\n')
    oracle.data('vasm-' + side + '-original-mask', vasm.masks[side])
  paths = [path for path in args.output.iterdir() if path.is_file()]
  paths += [args.models / 'lane.onnx', args.models / 'v_asm_model.onnx', *Path(cv2.__file__).parent.glob('*.so')]
  (args.output / 'fixtures.json').write_text(json.dumps({'cases': oracle.cases, 'wheel_defaults_preserved': True,
    'sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}}, indent=2) + '\n')


if __name__ == '__main__':
  main()
