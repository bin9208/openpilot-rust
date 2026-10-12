from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys
from types import SimpleNamespace

from cluster_simulator_compare import compare


class Device:
  def __init__(self, sample):
    self.sample = sample

  def get_axis(self, index):
    value = self.sample["axes"][index]
    return value if self.sample["kind"] == "controller" else value / 32768.0

  def get_button(self, index):
    return self.sample["buttons"][index]

  def get_numaxes(self):
    return len(self.sample["axes"])

  def get_numbuttons(self):
    return len(self.sample["buttons"])

  def get_numhats(self):
    return int("hat_x" in self.sample)

  def get_hat(self, index):
    assert index == 0
    return self.sample["hat_x"], 0


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_gamepad as original

  rng = random.Random(253)
  frames = []
  for index in range(480):
    controller = index % 2 == 0
    sample = {
      "kind": "controller" if controller else "joystick",
      "axes": [rng.randint(-32768, 32767) for _ in range(6 if controller else index % 7)],
      "buttons": [rng.choice([True, False]) for _ in range(15 if controller else index % 16)],
    }
    if not controller:
      sample["hat_x"] = [-1, 0, 1][index % 3]
    now = [10.0, 10.59999, 10.6, 10.60001, 12.0][index % 5]
    frames.append({"now": now, "sample": sample})
  axes = [
    -65536,
    -32768,
    -32767,
    -2,
    -1,
    -0.99,
    -0.94,
    -0.08,
    -0.06,
    -0.03,
    -0.0,
    0.0,
    0.01,
    0.02999,
    0.03,
    0.05999,
    0.06,
    0.07999,
    0.08,
    0.1,
    1.0,
    1.1,
    2.0,
    32767,
    32768,
    65536,
  ]
  instance = original.DualSenseSimulator.__new__(original.DualSenseSimulator)
  instance.warmup_until = 10.6
  instance.pygame = SimpleNamespace(
    event=SimpleNamespace(pump=lambda: None),
    CONTROLLER_AXIS_LEFTX=0,
    CONTROLLER_AXIS_RIGHTX=2,
    CONTROLLER_AXIS_RIGHTY=3,
    CONTROLLER_AXIS_TRIGGERLEFT=4,
    CONTROLLER_AXIS_TRIGGERRIGHT=5,
    CONTROLLER_BUTTON_DPAD_LEFT=13,
    CONTROLLER_BUTTON_DPAD_RIGHT=14,
    CONTROLLER_BUTTON_LEFTSHOULDER=9,
    CONTROLLER_BUTTON_RIGHTSHOULDER=10,
  )
  expected_frames = []
  for frame in frames:
    instance.using_controller_api = frame["sample"]["kind"] == "controller"
    device = Device(frame["sample"])
    instance.controller = device if instance.using_controller_api else None
    instance.joystick = None if instance.using_controller_api else device
    original.time = SimpleNamespace(perf_counter=lambda now=frame["now"]: now)
    expected_frames.append({"command": asdict(instance.read_input()), "status": instance.status_text()})
  functions = [original.normalize_signed_axis, original.normalize_trigger_axis, original.normalize_stick, original.normalize_view_axis]
  expected = {"frames": expected_frames, "axes": [[function(value) for function in functions] for value in axes]}
  request = {"start": 10.0, "frames": frames, "axes": axes}
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  before = hashlib.sha256(args.native.read_bytes()).hexdigest()
  result = subprocess.run([str(args.native)], input=json.dumps(request), text=True, capture_output=True, check=True)
  (args.output / "native.json").write_text(result.stdout)
  compare(expected, json.loads(result.stdout))
  assert before == hashlib.sha256(args.native.read_bytes()).hexdigest()
  receipt = {
    "frames": len(frames),
    "normalizations": len(axes) * 4,
    "provider_sha256": before,
    "source_sha256": hashlib.sha256((args.root / "openpilot/selfdrive/carrot/cluster/cluster_gamepad.py").read_bytes()).hexdigest(),
    "pygame_axis_source": "https://github.com/pygame/pygame/blob/2.6.1/src_c/joystick.c#L348",
    "seam": "raw SDL samples and clock injected into unchanged source read_input; native SDL opening/lifecycle is separate",
  }
  (args.output / "result.json").write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
