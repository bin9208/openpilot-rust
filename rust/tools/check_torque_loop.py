#!/usr/bin/env python3
"""Execute original torqued main-loop body against Rust state/controller steps."""

from __future__ import annotations
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import numpy as np
from check_message_state import source as messaging_source
from torque_cases import car_bytes, points
from torque_reference import Parameters, original, compare
from openpilot.cereal import car, log

TOPICS = ["carControl", "carOutput", "carState", "liveCalibration", "livePose", "liveDelay"]


def fixtures(new_message):
  for simulation, debug in ((False, False), (True, False), (True, True)):
    cp = car_bytes()
    cache = log.Event.new_message()
    saved = cache.init("liveTorqueParameters")
    saved.version, saved.decay = 1, 50.0
    saved.points = [[float(x), float(y)] for x, y in points()]
    for index in range(750):
      messages = []
      for topic in TOPICS:
        event = new_message(topic, valid=not 350 <= index < 390)
        event.logMonoTime = 100_000_000_000 + index * 50_000_000
        match topic:
          case "carControl":
            event.carControl.latActive = True
          case "carOutput":
            event.carOutput.actuatorsOutput.torque = 0.2
          case "carState":
            event.carState.vEgo = 20.0
          case "liveCalibration":
            event.liveCalibration.rpyCalib = [0.01, -0.02, 0.03]
          case "liveDelay":
            event.liveDelay.lateralDelay = 0.5 if index % 2 == 0 else 0.0
          case "livePose":
            pose = event.livePose
            pose.timestamp = event.logMonoTime
            pose.orientationNED.valid = pose.angularVelocityDevice.valid = True
            pose.angularVelocityDevice.z = -0.02
            pose.inputsOK = pose.sensorsOK = pose.posenetOK = True
        if (index < 20 or 150 <= index < 185 or index % 37 == 0) or (topic == "liveDelay" and 250 <= index < 300):
          continue
        messages.append(event)
      yield (
        {
          "configuration": {"car": list(cp), "saved": list(cache.to_bytes()), "simulation": simulation, "debug": debug} if index == 0 else None,
          "time": 100.0 + index * 0.05,
          "messages": [list(m.to_bytes()) for m in messages],
        },
        messages,
      )


def check(binary: Path, numerics: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  assert np.__version__ == "2.5.3", "source loop oracle must use the repository-locked NumPy2.5.3"
  scope, environment = messaging_source()
  counts = Counter()
  with subprocess.Popen([binary, numerics], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as process, (output / "trace.jsonl").open("w") as trace:
    assert process.stdin is not None and process.stdout is not None
    for request, messages in fixtures(scope["new_message"]):
      if request["configuration"]:
        config = request["configuration"]
        params = Parameters(bytes(config["car"]), bytes(config["saved"]))
        source = original(params)
        np.random.seed(42)  # noqa: NPY002 - source RandomState sampling is the compatibility contract.
        with car.CarParams.from_bytes(bytes(config["car"])) as cp:
          estimator = source.TorqueEstimator(cp, track_all_points=True)
        environment["simulation"] = "1" if config["simulation"] else "0"
        sm = scope["SubMaster"](TOPICS, poll="livePose")
        sent = []
        context = {
          **source.__dict__,
          "estimator": estimator,
          "sm": sm,
          "params": params,
          "DEBUG": config["debug"],
          "pm": SimpleNamespace(send=lambda topic, packet, sent=sent: sent.append(packet)),
        }
      before_sent, before_saved = len(sent), len(params.writes)
      sm.update = lambda sm=sm, request=request, messages=messages: sm.update_msgs(request["time"], [m.as_reader() for m in messages])
      exec(source.loop_body, context)
      process.stdin.write(json.dumps(request) + "\n")
      process.stdin.flush()
      actual = json.loads(process.stdout.readline())
      try:
        assert actual["frame"] == sm.frame and actual["valid"] == sm.all_checks()
        assert (actual["packet"] is not None) == (len(sent) > before_sent)
        assert (actual["persisted"] is not None) == (len(params.writes) > before_saved)
        assert actual["counts"] == [len(b) for b in estimator.filtered_points.buckets.values()]
        assert actual["admitted"] == len(estimator.all_torque_points)
        if actual["packet"] is not None:
          sent[-1].logMonoTime = 0
          with log.Event.from_bytes(bytes(actual["packet"])) as packet:
            counts["packet_fields"] += compare(packet.to_dict(), sent[-1].to_dict())
          counts["valid_publications" if actual["valid"] else "invalid_publications"] += 1
        if actual["persisted"] is not None:
          with log.Event.from_bytes(params.writes[-1]) as saved:
            wanted = saved.to_dict()
          wanted["logMonoTime"] = 0
          with log.Event.from_bytes(bytes(actual["persisted"])) as packet:
            counts["persisted_fields"] += compare(packet.to_dict(), wanted)
          counts["persistence_steps"] += 1
        counts["steps"] += 1
        trace.write(json.dumps(actual) + "\n")
      except AssertionError:
        (output / "failure.json").write_text(json.dumps({"request": request, "actual": actual}, indent=2) + "\n")
        raise
    process.stdin.close()
    assert process.wait(timeout=5) == 0
  assert counts["steps"] == 2250 and counts["persistence_steps"] == 12
  assert counts["valid_publications"] and counts["invalid_publications"]
  (output / "report.json").write_text(json.dumps({"result": "pass", "numpy": np.__version__, **counts}, indent=2) + "\n")
  print(json.dumps(counts, indent=2))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--numerics", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.numerics.resolve(), args.output.resolve())
