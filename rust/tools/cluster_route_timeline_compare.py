from __future__ import annotations

import argparse
from dataclasses import asdict, replace
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from cluster_route_frames_compare import native_frame
from cluster_simulator_compare import compare


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--log", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.root, args.output = args.root.resolve(), args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  base = source.RouteLogParser(recompute_cutins=False).parse_file(args.log.resolve(), source.load_openpilot_log_schema())[0]
  route = source.RouteReplaySource.__new__(source.RouteReplaySource)
  route.frames, route.times, route.duration = [], [], 0.0
  route.video_segments, route.wide_video_segments, route._loaded_chunks = [], [], []
  route._loaded_file_count, route._next_file_index = 0, 0
  route._end_of_route = True
  route.corner_yaw_comp_gain, route.corner_lateral_offset_m = 0.0, 0.0
  route._ensure_loaded = lambda _: None
  operations, expected = [], []
  for index in range(24):
    start = index * 10.0
    frames = (
      [] if index % 7 == 6 else [replace(base, t=start + offset, speed_kph=20.0 + index + item) for item, offset in enumerate([0.0, 0.0, 0.01, 1.0, 5.0, 10.0])]
    )
    path = args.output / f"route--{index}" / "rlog.zst"
    route._next_file_index = index + 1
    route._append_parsed_file(source.RouteReplayParsedFile(index, path, frames))
    operations.append({"op": "append", "index": index, "path": str(path), "frames": [native_frame(frame, source.frame_to_state) for frame in frames]})
    for trim in [start, start + 1.000001]:
      route._trim_loaded_chunks(trim)
      times = [start - 1.0, start - 0.5, start, start + 0.005, start + 0.5, start + 5.0, start + 10.0, start + 10.5, start + 20.0]
      operations.append({"op": "sample", "trim": trim, "times": times})
      samples = [
        {
          "state": asdict(route.state_at(time)),
          "status": native_frame(route._status_frame_at(time), source.frame_to_state),
          "path": str(route.log_path_at(time)),
          "offset": route.log_offset_at(time),
        }
        for time in times
      ]
      expected.append(
        {"chunks": len(route._loaded_chunks), "duration": route.duration, "first_time": route.times[0] if route.times else None, "samples": samples}
      )
  (args.output / "queries.json").write_text(json.dumps(operations))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"samples": len(expected) * 9, "batches": len(expected)}))
    return
  result = subprocess.run([str(args.native.resolve())], input="".join(json.dumps(operation) + "\n" for operation in operations), text=True, capture_output=True)
  (args.output / "native.jsonl").write_text(result.stdout)
  (args.output / "native.stderr").write_text(result.stderr)
  result.check_returncode()
  compare(json.loads(json.dumps(expected)), [json.loads(line) for line in result.stdout.splitlines()])
  report = {
    "samples": len(expected) * 9,
    "batches": len(expected),
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
