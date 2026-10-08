from __future__ import annotations

from dataclasses import replace
import hashlib
import json
from pathlib import Path
import time
from typing import Final

from openpilot.cereal import car
from openpilot.common.params import Params
from openpilot.selfdrive.carrot.xiaoge.nv12 import pack_nv12
from openpilot.selfdrive.carrot.xiaoge.v_asm_inference import VASMInference

from xiaoge_qa.live_input import HEIGHT, PIXELS, STRIDE, UV_OFFSET, WIDTH, Inputs, Phase
from xiaoge_qa.live_probe import Probe, side_ready, snapshots, stable_status, tcp_values

CONFIG: Final = {"width": 128, "height": 80,
  "poly_left": [[0, 20], [60, 20], [60, 79], [0, 79]],
  "poly_right": [[68, 20], [127, 20], [127, 79], [68, 79]]}


def exercise(probe: Probe, inputs: Inputs, tcp_port: int, output: Path):
  records = []

  def expected_confidence(status, config):
    model = VASMInference(Path(status["model"]["path"]))
    assert model.load(), model.error
    model.load_config(config)
    model.update(pack_nv12(PIXELS, WIDTH, HEIGHT, STRIDE, UV_OFFSET), WIDTH, HEIGHT, "left", .45, .2, 1.0)
    return model.confidence["left"]

  def record(name: str, status, *, publish: bool = True, empty: bool = False, tesla: bool = True):
    row = {"name": name, "status": stable_status(status), "tcp": tcp_values(tcp_port, empty, tesla, max(inputs.phase.speed, 0.0))}
    if publish:
      row["publication"] = probe.publication(status, publication_start)
    records.append(row)
    (output / "cases.json").write_text(json.dumps(records, indent=2) + "\n")
    print(name, "PASS", flush=True)

  publication_start = 0
  status = probe.await_status("params-directory", lambda value: side_ready(value, "left"))
  record("params-directory", status, tesla=False)
  car_params = Path(Params().get_param_path("CarParams"))
  car_params.rmdir()
  car_params.write_bytes(b"")
  minimum = status["inference"]["count"] + 2
  publication_start = len(probe.publications)
  status = probe.await_status("params-empty", lambda value: side_ready(value, "left", minimum))
  record("params-empty", status, tesla=False)
  cp = car.CarParams.new_message()
  cp.brand = "tesla"
  Params().put("CarParams", cp.to_bytes())
  publication_start = len(probe.publications)
  status = probe.await_status("left", lambda value: side_ready(value, "left"))
  record("left", status)
  snapshots(probe.connection, output)
  for name in ["wide", "road"]:
    records[-1][name + "_jpeg_sha256"] = hashlib.sha256((output / f"{name}.jpg").read_bytes()).hexdigest()
  for phase, reason in [
      (Phase("right", direction="right"), ""),
      (Phase("invalid", valid=False), "carState or modelV2 is unavailable"),
      (Phase("narrow", direction="right", lane_width=2.75), "target lane width below 3.0 m"),
      (Phase("negative-speed", speed=-3.0), "speed outside 30-120 km/h"),
      (Phase("high-speed", speed=34.0), "speed outside 30-120 km/h"),
      (Phase("no-direction", direction="none"), "no lane-change direction")]:
    publication_start = len(probe.publications)
    inputs.select(phase)
    if reason:
      status = probe.await_status(phase.name, lambda value, reason=reason: value["gate"]["reason"] == reason and not value["gate"]["active"])
    else:
      status = probe.await_status(phase.name, lambda value: side_ready(value, "right"))
    record(phase.name, status)
  publication_start = len(probe.publications)
  inputs.select(Phase("can-stale", can=False))
  status = probe.await_status("can-stale", lambda value: side_ready(value, "left"))
  record("can-stale", status, tesla=False)
  cp = car.CarParams.new_message()
  cp.brand = "hyundai"
  Params().put("CarParams", cp.to_bytes())
  publication_start = len(probe.publications)
  inputs.select(Phase("brand-latched"))
  status = probe.await_status("brand-latched", lambda value: side_ready(value, "left"))
  record("brand-latched", status)
  publication_start = len(probe.publications)
  inputs.select(Phase("cereal-stale", cereal=False))
  status = probe.await_status("cereal-stale", lambda value: value["gate"]["reason"] == "carState or modelV2 is unavailable")
  record("cereal-stale", status, empty=True)
  publication_start = len(probe.publications)
  inputs.select(Phase("recovered"))
  status = probe.await_status("recovered", lambda value: side_ready(value, "left"))
  record("recovered", status)
  Params().put("OnnxLaneThreshold", 73)
  Params().put("OnnxBsdIntervalMs", 600)
  status = probe.await_status("params-refresh", lambda value: value["lane"]["threshold"] == .73 and value["baseIntervalSeconds"] == .6)
  record("params-refresh", status)
  response = probe.connection.delete("/api/config")
  response.raise_for_status()
  default = probe.connection.get("/api/config")
  default.raise_for_status()
  assert default.json() != CONFIG and not (output / "config.json").exists()
  confidence = expected_confidence(status, default.json())
  publication_start = len(probe.publications)
  status = probe.await_status("config-cleared", lambda value: side_ready(value, "left") and value["vehicleSide"]["left"]["confidence"] == confidence)
  record("config-cleared", status)
  response = probe.connection.post("/api/config", json=CONFIG)
  response.raise_for_status()
  minimum = status["inference"]["count"] + 1
  confidence = expected_confidence(status, CONFIG)
  publication_start = len(probe.publications)
  status = probe.await_status("config-reloaded", lambda value: side_ready(value, "left", minimum) and value["vehicleSide"]["left"]["confidence"] == confidence)
  record("config-reloaded", status)
  inputs.select(Phase("camera-stale", camera=False))
  status = probe.await_status("camera-stale", lambda value: not value["camera"]["available"] and not value["lane"]["cameraAvailable"])
  assert not status["lane"]["resultFresh"] and not any(side["valid"] for side in status["vehicleSide"].values())
  counts = [status["inference"]["count"], status["lane"]["inference"]["count"]]
  deadline = time.monotonic() + .7
  while time.monotonic() < deadline:
    current = probe.status()
    assert [current["inference"]["count"], current["lane"]["inference"]["count"]] == counts
    time.sleep(.02)
  record("camera-stale", status, publish=False)
  inputs.select(Phase("camera-resumed"))
  publication_start = len(probe.publications)
  status = probe.await_status("camera-resumed", lambda value: side_ready(value, "left", counts[0] + 1))
  record("camera-resumed", status)
  inputs.select(replace(Phase("camera-restarted"), generation=1))
  minimum = status["inference"]["count"] + 2
  publication_start = len(probe.publications)
  status = probe.await_status("camera-restarted", lambda value: side_ready(value, "left", minimum))
  record("camera-restarted", status)
  return records
