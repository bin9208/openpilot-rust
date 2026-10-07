#!/usr/bin/env python3
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import random
import struct
import types

from joystickd_source import load_binding

ROOT = Path(__file__).resolve().parents[2]


def bits(value: float) -> int:
  return struct.unpack("<Q", struct.pack("<d", value))[0]


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--binding", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--joystick-only", action="store_true")
  args = parser.parse_args()
  load_binding(args.binding.resolve())
  from openpilot.tools.joystick import joystick_control as original
  modules = []
  if not args.joystick_only:
    from openpilot.tools.longitudinal_maneuvers import maneuversd as longitudinal
    from openpilot.tools.lateral_maneuvers import lateral_maneuversd as lateral
    modules = [("longitudinal", longitudinal), ("lateral", lateral)]

  args.output.mkdir(parents=True, exist_ok=False)
  requests, expected = [], []
  randomizer = random.Random(208)
  keys = ["w"] * 25 + ["s"] * 50 + ["a"] * 25 + ["d"] * 50 + ["c", "C", "r", "R", "q", "", "W", "A", "İ"]
  keys += [randomizer.choice("wwssaaddrRcCq123\n") for _ in range(500)]
  keyboard = types.SimpleNamespace(value="")
  keyboard.getch = lambda: keyboard.value
  original.KBHit = lambda: keyboard
  owner = original.Keyboard()
  rows = []
  for key in keys:
    keyboard.value = key
    result = owner.update()
    rows.append({"returned": result, "axes": [bits(owner.axes_values[name]) for name in owner.axes_order], "cancel": owner.cancel})
  requests.append({"kind": "keyboard", "keys": keys})
  expected.append(rows)
  for profile in ("pc", "tici"):
    original.HARDWARE = types.SimpleNamespace(get_device_type=lambda name=profile: name)
    owner = original.Joystick()
    events = [{"code": code, "state": state} for code in ("ABS_Z", "ABS_RX", "ABS_RY", "ABS_RZ", "BTN_NORTH", "SYN_REPORT")
              for state in (0, 1, 127, 128, 129, 255, -255, 256, -32768, 32767, -2147483648, 2147483647)]
    events += [{"code": randomizer.choice(("ABS_Z", "ABS_RX", "ABS_RY", "ABS_RZ", "BTN_NORTH", "REL_X")),
                "state": randomizer.randrange(-400, 400)} for _ in range(800)]
    events += [{"error": "unplugged"}, {"code": "BTN_NORTH", "state": 1}, {"error": "os"}, {"code": "ABS_Z", "state": 127}]
    rows = []
    for event in events:
      def read(current=event):
        if "error" in current:
          if current["error"] == "unplugged":
            raise original.UnpluggedError()
          raise OSError("owned fixture input failure")
        return [types.SimpleNamespace(**current)]
      original.get_gamepad = read
      result = owner.update()
      rows.append({"returned": result, "axes": [bits(owner.axes_values[name]) for name in owner.axes_order], "cancel": owner.cancel,
        "minimum": [bits(owner.min_axis_value[name]) for name in owner.axes_order],
        "maximum": [bits(owner.max_axis_value[name]) for name in owner.axes_order]})
    requests.append({"kind": "gamepad", "profile": profile, "events": events})
    expected.append(rows)
  for kind, module in modules:
    for template in module.MANEUVERS:
      owner = copy.deepcopy(template)
      actions = [{"accel": list(action.accel_bp), "time": list(action.time_bp)} for action in owner.actions]
      steps, rows = [], []
      maximum = int(sum(action.time_bp[-1] for action in owner.actions) / .05 + 100) * (owner.repeat + 1) + 300
      for frame in range(maximum):
        reset = frame in (32, 177)
        speed = owner.initial_speed + (1 if frame in range(15, 23) else 0)
        active = frame not in (2, 16, 54, 200)
        if reset:
          owner.reset()
        if kind == "longitudinal":
          standstill, cruise = frame != 7, frame in (6, 18)
          values = [speed, active, standstill, cruise]
        else:
          curvature = .002 if frame in range(3, 12) else -.001
          roll = .08 if frame in range(34, 40) else -.02
          values = [speed, active, curvature, roll]
        result = owner.get_accel(*values)
        steps.append({"reset": reset, "values": values})
        row = {"accel": bits(result), "active": owner.active, "finished": owner.finished,
          "run_completed": owner._run_completed, "action_index": owner._action_index,
          "action_frames": owner._action_frames, "ready_count": owner._ready_cnt, "repeated": owner._repeated}
        if kind == "lateral":
          row["baseline_curvature"] = bits(owner._baseline_curvature)
        rows.append(row)
        if owner.finished:
          break
      assert owner.finished, (kind, owner.description)
      requests.append({"kind": kind, "actions": actions, "repeat": owner.repeat, "initial_speed": owner.initial_speed, "steps": steps})
      expected.append(rows)
  (args.output / "input.json").write_text(json.dumps(requests, allow_nan=False) + "\n")
  (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
  sources = [Path(original.__file__), *(Path(module.__file__) for _, module in modules)]
  receipt = {"cases": len(requests), "steps": sum(map(len, expected)), "source_sha256": {
    str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}}
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
