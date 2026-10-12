from __future__ import annotations

import argparse
from dataclasses import replace
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

from cluster_route_frames_compare import native_frame


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
  queries, expected = [], []
  for index in range(120):
    number = [None, -0.0, 0.0, 0.125, 1.5, -2.5][index % 6]
    frame = replace(
      base,
      t=index / 10,
      speed_kph=index * 1.25,
      accel_mps2=-index / 20,
      steering_angle_deg=number,
      speed_limit_kph=None if index % 2 else 30,
      speed_limit_source=None if index % 3 else "carrot",
      cruise_kph=None if index % 3 else 42,
      gear_text="" if index % 2 else "D",
      cruise_gap=None if index % 2 else 2,
      lane_width_m=2.0 + index / 30,
      lane_center_offset_m=number,
      road_curvature=number,
      lane_change=None if index % 3 == 0 else "left",
      lane_change_progress=index / 120,
      planned_speed_kph=number,
      planned_accel_mps2=number,
      model_turn_speed_kph=number,
      engaged_prob=number,
      vision_speed_mps=number,
      vision_yaw_rate_rps=number,
      frame_drop_perc=number,
      model_execution_time_ms=number,
      model_confidence="" if index % 3 else "green",
      should_stop=index % 2 == 0,
    )
    vehicles = tuple(
      source.DetectedVehicle(
        f"차{i}",
        5.0 + i,
        (-1) ** i * 2.5,
        "radarState",
        probability=[1.0, 0.995, 0.125][i % 3],
        relative_speed_mps=None if i % 2 else -2.5,
        ttc_s=None if i % 3 else 3.25,
        cut_in=i % 2 == 0,
      )
      for i in range(index % 7)
    )
    points = tuple(
      source.RadarPoint(f"CR{i}", 10.0 - i, (-1) ** i * 2.5, "cornerRadar", probability=number, relative_speed_mps=number) for i in range(index % 5)
    )
    frame = replace(frame, detected_vehicles=vehicles, radar_points=points)
    route = source.RouteReplaySource.__new__(source.RouteReplaySource)
    route.duration = 20.0
    route._video_stream = "road"
    route.video_segments = [source.RouteVideoSegment(index % 3, args.output / "video", 0.0, 20.0)] if index % 2 else []
    route.wide_video_segments = []
    route._video_reader = SimpleNamespace(frame_at=lambda _: None, status_text=lambda: "road camera unavailable")
    playback, loop = (index - 20) / 2, index % 3 == 0
    overlay = route._route_overlay(frame, source.frame_to_state(frame), playback, loop, source.CLUSTER_CAMERA_VIEW_MODE_ROAD_CAMERA)
    queries.append(
      {
        "frame": native_frame(frame, source.frame_to_state),
        "shown_time": source.clamp(playback % 20 if loop else playback, 0, 20),
        "duration": 20.0,
        "segment_label": str(index % 3) if index % 2 else "--",
      }
    )
    expected.append(overlay.data_lines)
  (args.output / "queries.json").write_text(json.dumps(queries))
  (args.output / "source.json").write_text(json.dumps(expected))
  if args.native is None:
    print(json.dumps({"overlays": len(queries), "lines": len(queries) * 12}))
    return
  run = subprocess.run([str(args.native.resolve())], input="".join(json.dumps(query) + "\n" for query in queries), text=True, capture_output=True)
  (args.output / "native.jsonl").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  failures = [
    {"case": case, "line": index, "source": left, "native": right}
    for case, (left_lines, right_lines) in enumerate(zip(expected, actual, strict=True))
    for index, (left, right) in enumerate(zip(left_lines, right_lines, strict=True))
    if left != right
  ]
  report = {
    "overlays": len(queries),
    "lines": len(queries) * 12,
    "failures": failures,
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({**report, "failures": failures[:4], "failure_count": len(failures)}))
  assert not failures


if __name__ == "__main__":
  main()
