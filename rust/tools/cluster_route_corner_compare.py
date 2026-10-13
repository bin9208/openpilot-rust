from __future__ import annotations

import argparse
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys

from cluster_simulator_compare import compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.root = args.root.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  rng = random.Random(25302)
  queries, expected = [], []
  for address in [0x17F, *range(0x180, 0x186), 0x234, *range(0x235, 0x24A)]:
    for index in range(12):
      data = bytes([0 if index == 0 else 255 if index == 1 else rng.randrange(256) for _ in range([31, 32, 32, 33][index % 4])])
      t, speed = index * 0.05, index * 9.3
      objects = source.decode_raw_corner_objects(t, address, data)
      queries.append({"op": "decode", "t": t, "address": address, "data": list(data), "speed": speed})
      expected.append(
        {
          "objects": [asdict(obj) for obj in objects],
          "valid": [source.raw_corner_object_is_valid(obj) for obj in objects],
          "points": [asdict(source.raw_corner_object_to_radar_point(obj, speed)) for obj in objects],
        }
      )
  for sequence in range(8):
    tracker = source.StableCornerObjectTracker()
    objects, frames = [], []
    for index in range(120):
      for lane in range(3):
        obj = source.RawCornerObject(
          t=index * 0.05 + (0.7 if index >= 90 else 0),
          group="235" if lane < 2 else "180",
          address=0x235,
          slot=lane,
          quality=20 + index % 100,
          age=(index + 250) % 256,
          object_id=(lane if sequence % 2 else 3) + (10 if 40 <= index < 50 else 0),
          object_class=1,
          width=1.2,
          x=25.0 + lane * 20 - index * 0.08 + rng.uniform(-0.2, 0.2),
          y=[-3.0, 3.0, 9.0][lane] + rng.uniform(-0.1, 0.1),
          vx=-1.6,
          vy=0.1,
          ax=0.2,
        )
        if sequence == 7 and index > 110:
          obj = source.replace(obj, t=obj.t - 0.3)
        objects.append(asdict(obj))
        tracker.update(obj)
        frames.append([asdict(point) for point in tracker.points_at(obj.t, 72.0)])
    times = [objects[-1]["t"] + offset for offset in [0.0, 0.1, 0.55, 0.550001]]
    frames.extend([[asdict(point) for point in tracker.points_at(time, 72.0)] for time in times])
    queries.append({"op": "track", "objects": objects, "times": times, "speed": 72.0})
    expected.append(frames)
  for index in range(120):
    points = tuple(
      source.RadarPoint(label, rng.uniform(-1, 181), rng.uniform(-13, 13), kind, lateral_speed_mps=None if index % 3 == 0 else 1.2)
      for label, kind in [("CO001", "cornerRadar"), ("CR01", "cornerRadar"), ("R01", "cornerRadar"), ("CO002", "liveTracks")]
    )
    yaw = [None, 0.0, 0.349999, 0.35, -0.1][index % 5]
    gain, offset, width = [0.0, 0.6, -3.0][index % 3], [0.0, 0.5, -0.7][index % 3], rng.uniform(1, 6)
    queries.append({"op": "adjust", "points": [asdict(point) for point in points], "yaw": yaw, "gain": gain, "offset": offset, "width": width})
    expected.append(
      [asdict(point) for point in source.sorted_radar_points(source.replay_corner_adjusted_point(point, yaw, gain, offset, width) for point in points)]
    )
  (args.output / "queries.json").write_text(json.dumps(queries))
  (args.output / "source.json").write_text(json.dumps(expected))
  digest = hashlib.sha256(args.native.read_bytes()).hexdigest()
  result = subprocess.run([str(args.native.resolve())], input="".join(json.dumps(query) + "\n" for query in queries), capture_output=True, text=True)
  (args.output / "native.jsonl").write_text(result.stdout)
  (args.output / "native.stderr").write_text(result.stderr)
  result.check_returncode()
  compare(json.loads(json.dumps(expected)), [json.loads(line) for line in result.stdout.splitlines()])
  assert digest == hashlib.sha256(args.native.read_bytes()).hexdigest()
  report = {
    "queries": len(queries),
    "tracker_observations": 2880,
    "native_sha256": digest,
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
    "float_tolerance": 2e-14,
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
