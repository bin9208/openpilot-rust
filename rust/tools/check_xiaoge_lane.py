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

import numpy as np
import numpy.typing as npt

from openpilot.selfdrive.carrot.xiaoge import lane_inference as original

FloatArray = npt.NDArray[np.float32]


class LaneResult(TypedDict):
  leftLine: int
  rightLine: int
  leftConf: float
  rightConf: float
  valid: bool
  error: str
  candidatesCount: int


class Candidate(TypedDict):
  class_id: int
  score: float
  bottom: int
  center: float


class Expected(TypedDict):
  candidates: list[Candidate]
  result: LaneResult


class TensorNet:
  def __init__(self, predictions: FloatArray, prototypes: FloatArray):
    self.outputs = [predictions[None], prototypes[None]]

  def setInput(self, blob: FloatArray) -> None:
    assert blob.shape == (1, 3, 416, 416)

  def getUnconnectedOutLayersNames(self) -> list[str]:
    return ["output0", "output1"]

  def forward(self, names: list[str]) -> list[FloatArray]:
    assert names == ["output0", "output1"]
    return self.outputs


def fixture(seed: int) -> tuple[FloatArray, FloatArray]:
  rng = np.random.default_rng(seed)
  anchors = 96
  predictions = np.zeros((42, anchors), dtype=np.float32)
  predictions[:2] = rng.uniform(-100, 520, (2, anchors))
  predictions[2:4] = rng.uniform(-2, 250, (2, anchors))
  predictions[4:10] = rng.uniform(0, 1, (6, anchors))
  predictions[10:42] = rng.normal(0, 1, (32, anchors))
  prototypes = rng.normal(0, 1, (32, 104, 104)).astype(np.float32)
  match seed:
    case 0:
      predictions[4:10] = 0
    case 1:
      prototypes.fill(0)
    case 2:
      predictions[4:10] = 0.5
    case 3:
      predictions[:4] = np.array([200, 300, 160, 100], dtype=np.float32)[:, None]
    case 4:
      predictions[4:10, ::4] = np.nan
    case 5:
      predictions[4:10] = 0
      predictions[7:9] = 1
    case 6 | 7 | 8 | 9 | 10 | 11:
      pass
    case 12 | 13:
      predictions[4:10] = 0
      predictions[4, 0] = 0.9
      predictions[:4, 0] = [208, 208, 416, 416]
      predictions[10:, 0] = 1
      prototypes.fill(0)
      values = [1e8, 1, -1e8, 1] if seed == 12 else [1e8, -1, -1e8, 1]
      prototypes[:4] = np.array(values, dtype=np.float32)[:, None, None]
    case unexpected:
      raise AssertionError(unexpected)
  return predictions, prototypes


def source_result(predictions: FloatArray, prototypes: FloatArray) -> Expected:
  model = original.LaneInference()
  model.valid = True
  model.net = TensorNet(predictions, prototypes)
  captured = []
  selector = original.select_lane_results
  def capture(candidates):
    captured.extend({"class_id": candidate.class_id, "score": candidate.score,
                     "bottom": int(candidate.bottom), "center": float(candidate.center_at_bottom)} for candidate in candidates)
    return selector(candidates)
  original.select_lane_results = capture
  try:
    result = model.infer(np.zeros((416, 416), dtype=np.uint8), 416, 416)
  finally:
    original.select_lane_results = selector
  return {"candidates": captured, "result": result}


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare Rust lane postprocessing with the original NumPy implementation.")
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  assert shutil.disk_usage(args.output.parent).free > 25 * 2**30 + 64 * 2**20
  args.output.mkdir(parents=True, exist_ok=False)
  inputs = []
  expected = []
  for seed in range(14):
    predictions, prototypes = fixture(seed)
    prediction_path = (args.output / f"predictions-{seed}.f32").resolve()
    prototype_path = (args.output / f"prototypes-{seed}.f32").resolve()
    predictions.astype("<f4").tofile(prediction_path)
    prototypes.astype("<f4").tofile(prototype_path)
    inputs.append({"predictions": str(prediction_path), "prototypes": str(prototype_path), "confidence": 0.25, "iou": 0.5})
    expected.append(source_result(predictions, prototypes))
  payload = json.dumps(inputs).encode()
  (args.output / "input.json").write_bytes(payload)
  (args.output / "source.json").write_text(json.dumps(expected, indent=2) + "\n")
  run = subprocess.run([args.binary.resolve()], input=payload, capture_output=True, check=False)
  (args.output / "native.json").write_bytes(run.stdout)
  (args.output / "native.stderr").write_bytes(run.stderr)
  run.check_returncode()
  actual = json.loads(run.stdout)
  assert len(actual) == len(expected)
  for index, (wanted, got) in enumerate(zip(expected, actual, strict=True)):
    assert wanted == got, (index, wanted, got)
  evidence = {"status": "PASS", "cases": len(inputs), "scope": "original postprocessing with injected DNN output; model inference tested separately",
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(original.__file__).read_bytes()).hexdigest()}
  (args.output / "receipt.json").write_text(json.dumps(evidence, indent=2) + "\n")
  print(json.dumps(evidence))


if __name__ == "__main__":
  main()
