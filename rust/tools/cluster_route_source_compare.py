from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from cluster_simulator_compare import compare


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--launcher", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  cli.add_argument("--front-radar-only", choices=["0", "1"])
  args = cli.parse_args()
  args.root, args.output = args.root.resolve(), args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  if args.front_radar_only is not None:
    os.environ["CLUSTER_ROUTE_FRONT_RADAR_ONLY"] = args.front_radar_only
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  schema = source.load_openpilot_log_schema()
  files = []
  for index in range(6):
    path = args.output / f"route--{index}" / "rlog.raw"
    path.parent.mkdir(exist_ok=True)
    events = []
    if args.front_radar_only is not None:
      event = schema.Event.new_message(logMonoTime=(100 + index * 60) * 1_000_000_000, valid=True)
      event.init("carParams").extFlags = 1 << 12
      events.append(event.to_bytes())
    for frame in range(60):
      event = schema.Event.new_message(logMonoTime=(100 + index * 60 + frame) * 1_000_000_000, valid=True)
      car = event.init("carState")
      car.vEgo, car.aEgo, car.vEgoCluster = 10.0 + index + frame / 20, 0.05, 10.0 + index
      car.leftBlinker, car.rightBlinker = 20 <= frame < 30, 40 <= frame < 45
      events.append(event.to_bytes())
    path.write_bytes(b"".join(events))
    files.append(path)
  steps = [{"time": float(time), "repeat": False} for time in [0, 1, 10, 65, 70, 125, 5, 65, 125, 185, 245, 305, 365, 5]]
  steps.extend({"time": float(time), "repeat": True} for time in [360, 365, 425, 721])
  expected = []
  route = source.RouteReplaySource(files)
  try:
    for step in steps:
      time, repeat = step["time"], step["repeat"]
      state = route.state_at(time, repeat, True, 1)
      expected.append(
        {
          "state": asdict(state),
          "status": route.status_text(time, repeat),
          "duration": route.duration,
          "loaded": route.loaded_file_count,
          "path": str(route.log_path_at(time)),
          "folder": str(route.log_folder_at(time)),
          "offset": route.log_offset_at(time),
          "finished": route.is_finished(time, repeat),
        }
      )
  finally:
    route.close()
    route.close()
  request = {"files": list(map(str, files)), "steps": steps, "launcher": str(args.launcher)}
  if args.front_radar_only is not None:
    request["front_radar_only"] = args.front_radar_only == "1"
  (args.output / "request.json").write_text(json.dumps(request))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"samples": len(steps), "files": len(files)}))
    return
  env = os.environ.copy()
  if args.front_radar_only is not None:
    env["CLUSTER_ROUTE_FRONT_RADAR_ONLY"] = "0" if args.front_radar_only == "1" else "1"
  run = subprocess.run([str(args.native.resolve())], input=json.dumps(request), text=True, capture_output=True, timeout=30, env=env)
  (args.output / "native.json").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  compare(json.loads(json.dumps(expected)), json.loads(run.stdout))
  report = {
    "samples": len(steps),
    "files": len(files),
    "explicit_front_radar_only": args.front_radar_only,
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
