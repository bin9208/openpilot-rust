from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from cluster_route_frames_compare import native_frame
from cluster_simulator_compare import compare


def raw_payload(quality, age):
  value = 0
  for start, width, raw in [(24, 7, quality), (32, 8, age), (44, 7, 3), (64, 13, 600), (78, 12, 2108), (91, 12, 1980), (104, 10, 500)]:
    value |= (raw & ((1 << width) - 1)) << start
  return value.to_bytes(32, "little")


def corner_payload(source, address, distance):
  value = 0
  for name, spec in source.hyundai_canfd_corner_dbc_signals()[address].items():
    physical = 1 if name.endswith("_DETECT") else distance if name.endswith("_DISTANCE") else 3.0 if name.endswith("_LATERAL") else 0
    raw = round((physical - spec.offset) / spec.factor)
    if spec.byte_order == "le":
      value |= (raw & ((1 << spec.length) - 1)) << spec.start
    else:
      bit = spec.start
      for offset in range(spec.length):
        value |= ((raw >> (spec.length - offset - 1)) & 1) << bit
        bit = bit + 15 if bit % 8 == 0 else bit - 1
  return value.to_bytes(32, "little")


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.root, args.output = args.root.resolve(), args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  schema = source.load_openpilot_log_schema()
  events = []
  for index in range(100):
    time = 1 + index * 0.1
    if index < 18 or 30 <= index < 42:
      event = schema.Event.new_message(logMonoTime=round(time * 1e9), valid=True)
      service = "sendcan" if index == 4 else "can"
      messages = event.init(service, 1)
      messages[0].address = 0x1EA if index < 10 else 0x162
      messages[0].src = [2, 6, 130, 0][index % 4]
      messages[0].dat = corner_payload(source, messages[0].address, 20 + index)[: 23 if index == 8 else 32]
      events.append(event.to_bytes())
    if 50 <= index < 75:
      event = schema.Event.new_message(logMonoTime=round(time * 1e9), valid=True)
      message = event.init("can", 1)[0]
      message.address = 0x235
      message.src = 1 if index != 70 else 129
      message.dat = raw_payload(0 if index == 63 else 20, index)
      events.append(event.to_bytes())
    if index in [48, 52, 60, 80]:
      event = schema.Event.new_message(logMonoTime=round(time * 1e9), valid=True)
      point = event.init("liveTracks").init("points", 1)[0]
      point.trackId, point.dRel, point.yRel, point.vRel, point.vLead, point.measured = 1, 30.0, -3.0, -1.0, 19.0, True
      events.append(event.to_bytes())
    event = schema.Event.new_message(logMonoTime=round((time + 0.01) * 1e9), valid=True)
    car = event.init("carState")
    car.vEgo, car.leftBlindspot, car.rightBlindspot = 20.0, index % 3 == 0, index % 4 == 0
    car.leftLongDist, car.leftLatDist = (35.0 if index % 7 == 0 else 0.0), 3.0
    events.append(event.to_bytes())
  path = args.output / "can.raw"
  path.write_bytes(b"".join(events))
  failures = []
  for front in [False, True]:
    os.environ["CLUSTER_ROUTE_FRONT_RADAR_ONLY"] = "1" if front else "0"
    for corner in ["live", "raw", "stable"]:
      key = f"{corner}-front-{front}"
      original = source.RouteLogParser(corner, recompute_cutins=False)
      expected = [[native_frame(frame, source.frame_to_state) for frame in original.parse_file(path, schema)]]
      (args.output / f"{key}-source.json").write_text(json.dumps(expected))
      request = {"mode": "parse", "paths": [str(path)], "corner": corner}
      run = subprocess.run([str(args.native.resolve())], input=json.dumps(request), text=True, capture_output=True)
      (args.output / f"{key}-native.json").write_text(run.stdout)
      (args.output / f"{key}-native.stderr").write_text(run.stderr)
      run.check_returncode()
      actual = json.loads(run.stdout)
      for index, (left, right) in enumerate(zip(expected[0], actual[0], strict=True)):
        try:
          compare(json.loads(json.dumps(left)), right)
        except AssertionError as error:
          failures.append({"case": key, "frame": index, "detail": str(error)})
  report = {
    "frames": 600,
    "failures": failures,
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({**report, "failures": len(failures), "first": failures[:3]}))
  assert not failures


if __name__ == "__main__":
  main()
