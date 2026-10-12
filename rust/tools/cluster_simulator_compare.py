from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import sys


def compare(left, right, where="root"):
  if isinstance(left, dict):
    assert left.keys() == right.keys(), (where, left.keys() - right.keys(), right.keys() - left.keys())
    for key in left:
      compare(left[key], right[key], f"{where}.{key}")
  elif isinstance(left, (list, tuple)):
    assert len(left) == len(right), (where, len(left), len(right))
    for index, (expected, actual) in enumerate(zip(left, right, strict=True)):
      compare(expected, actual, f"{where}[{index}]")
  elif isinstance(left, float):
    assert isinstance(right, (float, int)) and math.isclose(left, right, rel_tol=2e-14, abs_tol=2e-14), (where, left, right)
  else:
    assert left == right, (where, left, right)


def make_cases(command_type):
  rng = random.Random(253)
  sequences = []
  for direction in ["left", "right", "both", "held"]:
    steps = []
    for index in range(240):
      command = command_type(
        throttle=0.5 if index < 120 else 0.0,
        brake=0.5 if index > 200 else 0.0,
        left_signal_requested=(direction in ["left", "both"] and index == 0) or direction == "held",
        right_signal_requested=direction in ["right", "both"] and index == 0,
      )
      steps.append({"dt": 0.05, "command": asdict(command)})
    sequences.append(steps)
  steps = []
  for index in range(720):
    command = command_type(
      throttle=rng.uniform(-0.3, 1.3),
      brake=rng.uniform(-0.3, 1.3),
      steering=rng.uniform(-2, 2),
      steering_angle_deg=None if index % 3 else rng.uniform(-90, 90),
      camera_lane_width_m=rng.uniform(1, 6),
      camera_lane_center_offset_m=None if index % 5 == 0 else rng.uniform(-8, 8),
      surround_yaw_deg=rng.uniform(-240, 240),
      surround_pitch_deg=rng.uniform(-30, 30),
      surround_view_active=index % 7 < 3,
      left_signal_requested=index % 101 == 0,
      right_signal_requested=index % 109 == 0,
    )
    steps.append({"dt": [0.0, -1.0, 0.001, 0.02, 0.1, 0.25, 1.0][index % 7], "command": asdict(command)})
  sequences.append(steps)
  sequences.append(
    [{"dt": 0.25, "command": asdict(command_type(throttle=1.0 if index < 1200 else 0.0, brake=1.0 if index >= 1200 else 0.0))} for index in range(1440)]
  )
  random_steps = [0.05] * 1200 + [0.0, -0.1, 100.0, 0.01, 8.0]
  return sequences, random_steps


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_simulator as original

  sequences, random_steps = make_cases(original.SimulatorInput)
  expected_sequences = []
  for steps in sequences:
    simulator = original.ClusterSimulator()
    expected_sequences.append(
      [{"ui": asdict(simulator.update(original.SimulatorInput(**step["command"]), step["dt"])), "internal": vars(simulator).copy()} for step in steps]
    )
  source = original.RandomInputSource()
  simulator = original.ClusterSimulator()
  expected_random = []
  for dt in random_steps:
    command = source.update(dt)
    expected_random.append(
      {"command": asdict(command), "input_internal": vars(source).copy(), "ui": asdict(simulator.update(command, dt)), "internal": vars(simulator).copy()}
    )
  request = {"sequences": sequences, "random_steps": random_steps}
  expected = {"sequences": expected_sequences, "random": expected_random}
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  before = hashlib.sha256(args.native.read_bytes()).hexdigest()
  result = subprocess.run([str(args.native)], input=json.dumps(request), text=True, capture_output=True, check=True)
  (args.output / "native.json").write_text(result.stdout)
  compare(expected, json.loads(result.stdout))
  assert before == hashlib.sha256(args.native.read_bytes()).hexdigest()
  receipt = {
    "manual_steps": sum(map(len, sequences)),
    "random_steps": len(random_steps),
    "provider_sha256": before,
    "float_relative_tolerance": 2e-14,
    "float_absolute_tolerance": 2e-14,
    "source_files": {
      name: hashlib.sha256((args.root / "openpilot/selfdrive/carrot/cluster" / name).read_bytes()).hexdigest()
      for name in ["cluster_simulator.py", "cluster_models.py", "cluster_config.py", "cluster_utils.py"]
    },
    "worktree_sources_at_comparison": {
      str(path.relative_to(args.root)): hashlib.sha256(path.read_bytes()).hexdigest()
      for path in sorted((args.root / "rust/crates/cluster/src/simulator").glob("*.rs"))
    },
  }
  (args.output / "result.json").write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
