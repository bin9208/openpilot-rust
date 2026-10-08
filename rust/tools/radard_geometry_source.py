import argparse
import dataclasses
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import struct

from openpilot.selfdrive.carrot.radar_motion.predictor import model_path_point_at_s, project_to_model_path, radar_target_velocity_in_ego_frame
from openpilot.selfdrive.carrot.radar_motion.primary import FrontRadarKinematicAssociator, RadarPointSnapshot
from plannerd_owner_source import difference


def fixtures():
  randomizer = random.Random(207)
  paths = [[(0.0, 0.0)], [(0.0, 0.0), (0.0, 0.0)], [(0.0, 0.0), (100.0, 0.0)]]
  for _ in range(125):
    paths.append([(index * randomizer.uniform(2.0, 4.0), randomizer.uniform(-10.0, 10.0)) for index in range(33)])
  for index, path in enumerate(paths):
    steps = []
    for frame in range(8):
      values = [
        RadarPointSnapshot(1, "frontRadar", 25.0, 0.1, -1.0, 0.2, 0.0, 15.0, -0.3, -0.2, True),
        RadarPointSnapshot(2, "frontRadar", 28.0, 0.5, -1.2, 0.0, -0.4, 14.8, 0.0, 0.0, True),
        RadarPointSnapshot(3501, "corner235", 24.8 + frame * 0.4, 0.1 + frame * 0.22, -1.1, 0.4, -0.2, 14.9, -0.4, 0.0, True),
        RadarPointSnapshot(1201, "corner180", 28.0, 0.5, -1.2, 0.0, -0.3, 14.8, -0.2, 0.0, True),
      ]
      if frame == 3:
        values = values[1:]
      if frame == 5:
        values = list(reversed(values))
      if frame == 7:
        values = []
      steps.append(values)
    yield {
      "path": path,
      "queries": [(randomizer.uniform(-10.0, 150.0), randomizer.uniform(-15.0, 15.0)) for _ in range(12)],
      "samples": [(randomizer.uniform(-10.0, 150.0), randomizer.uniform(-3.0, 3.0)) for _ in range(12)],
      "steps": steps,
      "yaw": (index - 64) * 0.01,
      "norms": [[randomizer.uniform(-100.0, 100.0) for _ in range(index % 3 + 1)],
                [1e308, -1e308], [5e-324, 1e-323, 2e-323], [math.nan, math.inf], [math.nan, 2.0], [-0.0, 0.0]],
    }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  requests, expected = [], []
  for request in fixtures():
    path = request["path"]
    associator = FrontRadarKinematicAssociator()
    steps = []
    for points in request["steps"]:
      matches = associator.update(points)
      steps.append({
        "matches": [[list(key), dataclasses.asdict(point)] for key, point in matches.items()],
        "pairs": [[list(key), list(value)] for key, value in associator._pairs.items()],
        "velocities": [list(radar_target_velocity_in_ego_frame(point.v_lead, point.yv_rel, point.d_rel, point.y_rel, request["yaw"])) for point in points],
      })
    expected.append({
      "projections": [dataclasses.asdict(project_to_model_path(path, *query)) for query in request["queries"]],
      "samples": [list(model_path_point_at_s(path, *sample)) for sample in request["samples"]],
      "steps": steps,
      "norms": [struct.unpack("<Q", struct.pack("<d", math.hypot(*values)))[0] for values in request["norms"]],
    })
    requests.append({**request, "steps": [[dataclasses.asdict(point) for point in points] for points in request["steps"]],
                     "norms": [[struct.unpack("<Q", struct.pack("<d", value))[0] for value in values] for values in request["norms"]]})
  encoded = json.dumps(requests, allow_nan=False)
  (args.output / "input.json").write_text(encoded + "\n")
  (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False) + "\n")
  child = subprocess.run([str(args.binary.resolve())], input=encoded, text=True, capture_output=True, check=False)
  (args.output / "actual.json").write_text(child.stdout)
  (args.output / "stderr.txt").write_text(child.stderr)
  child.check_returncode()
  mismatches = difference(expected, json.loads(child.stdout))
  receipt = {"cases": len(requests), "differences": len(mismatches), "first_differences": mismatches[:100],
             "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  assert not mismatches, receipt
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
