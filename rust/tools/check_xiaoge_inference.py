#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "opencv-python-headless==4.13.0.92"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from typing import TypedDict

import cv2
import numpy as np
from numpy.typing import NDArray

from openpilot.selfdrive.carrot.xiaoge.lane_inference import LaneInference, prepare_lane_image
from openpilot.selfdrive.carrot.xiaoge.nv12 import nv12_y_plane, pack_nv12
from openpilot.selfdrive.carrot.xiaoge.v_asm_inference import VASMInference


class Config(TypedDict):
  width: int
  height: int
  poly_left: list[list[int]]
  poly_right: list[list[int]]


class Input(TypedDict):
  frame: str
  width: int
  height: int
  stride: int
  uv_offset: int
  config: Config
  models: str
  output: str


class CapturedNet:
  def __init__(self, net: cv2.dnn.Net, blob: Path):
    self.net = net
    self.blob = blob

  def setInput(self, blob: NDArray[np.float32]) -> None:
    blob.astype("<f4").tofile(self.blob)
    self.net.setInput(blob)

  def forward(self) -> NDArray[np.float32]:
    return self.net.forward()


def source(request: Input, destination: Path):
  destination.mkdir()
  data = Path(request["frame"]).read_bytes()
  width, height, stride, uv_offset = (request[key] for key in ("width", "height", "stride", "uv_offset"))
  gray = nv12_y_plane(data, width, height, stride)
  packed = pack_nv12(data, width, height, stride, uv_offset)
  lane = LaneInference(Path(request["models"]) / "lane.onnx")
  assert lane.load(), lane.error
  prepare_lane_image(gray, width, height).tofile(destination / "lane-gray.bin")
  lane.preprocess_image(gray, width, height).astype("<f4").tofile(destination / "lane-blob.f32")
  lane_result = lane.infer(gray, width, height)
  model = VASMInference(Path(request["models"]) / "v_asm_model.onnx")
  assert model.load(), model.error
  model.load_config(request["config"])
  net = model.net
  sides = []
  for side in model.configured_sides:
    model.net = CapturedNet(net, destination / f"{side}-blob.f32")
    model.update(packed, width, height, side, 0.45, 0.2, 0.25)
    model.masks[side].tofile(destination / f"{side}-mask.bin")
    sides.append({"side": side, "bounds": list(map(int, model.bboxes[side])),
                  "detection": {"score": model.scores[side], "active": model.active[side], "confidence": model.confidence[side]}})
  return {"lane": lane_result, "sides": sides, "lane_loaded": lane.valid, "blindspot_loaded": model.valid}


def generate(output: Path, models: Path) -> list[Input]:
  rng = np.random.default_rng(201)
  config: Config = {"width": 1928, "height": 1208,
    "poly_left": [[0, 550], [550, 480], [650, 950], [0, 1200]],
    "poly_right": [[1378, 480], [1927, 550], [1927, 1200], [1278, 950]]}
  inputs = []
  for index, (width, height, padding, gap) in enumerate([(64, 48, 16, 8), (1280, 720, 32, 256), (1928, 1208, 48, 1024), (48, 64, 2, 12)]):
    stride = width + padding
    offset = stride * height + gap
    frame = rng.integers(0, 256, offset + stride * height // 2 + 7, dtype=np.uint8)
    frame_path = output / f"frame-{index}.bin"
    frame.tofile(frame_path)
    inputs.append(Input(frame=str(frame_path), width=width, height=height, stride=stride, uv_offset=offset,
                        config=config, models=str(models), output=str(output / f"native-{index}")))
  return inputs


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare complete Rust Xiaoge NV12-to-model-result execution with the original implementation.")
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--models", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  assert shutil.disk_usage(args.output.parent).free > 25 * 2**30 + 256 * 2**20
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(2)
  inputs = generate(args.output, args.models.resolve())
  expected = [source(request, args.output / f"source-{index}") for index, request in enumerate(inputs)]
  payload = json.dumps(inputs).encode()
  (args.output / "inputs.json").write_bytes(payload)
  (args.output / "source.json").write_text(json.dumps(expected, indent=2) + "\n")
  run = subprocess.run([args.binary.resolve()], input=payload, capture_output=True, check=False)
  (args.output / "native.json").write_bytes(run.stdout)
  (args.output / "native.stderr").write_bytes(run.stderr)
  run.check_returncode()
  actual = json.loads(run.stdout)
  assert len(actual) == len(expected)
  compared_files = 0
  for index, (wanted, got) in enumerate(zip(expected, actual, strict=True)):
    assert wanted == got, (index, wanted, got)
    for original in sorted((args.output / f"source-{index}").iterdir()):
      candidate = args.output / f"native-{index}" / original.name
      assert original.read_bytes() == candidate.read_bytes(), (index, original.name)
      compared_files += 1
  receipt = {"status": "PASS", "frames": len(inputs), "exact_images_and_tensors": compared_files,
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "scope": "host original vs Rust complete NV12 preprocessing, actual ONNX inference, lane and blindspot decisions"}
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
