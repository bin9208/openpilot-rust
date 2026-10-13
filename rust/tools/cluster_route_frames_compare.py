from __future__ import annotations

import argparse
import ast
from dataclasses import asdict, dataclass, fields, replace
import hashlib
import json
import math
from pathlib import Path
import random
import subprocess
import sys
from typing import Any

from cluster_simulator_compare import compare


def original(root):
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_config
  import cluster_models
  import cluster_simulator
  import cluster_utils

  path = root / "openpilot/selfdrive/carrot/cluster/cluster_route_replay.py"
  source = ast.parse(path.read_text())
  names = {
    "RouteReplayFrame",
    "frame_to_state",
    "route_lane_animation_values",
    "shifted_optional_offset",
    "blend_frames",
    "lanes_for_frame",
    "model_lane_offset_for_index",
    "model_lane_color_for_index",
    "model_lane_style_for_index",
    "model_line_at",
    "model_line_lateral_shift",
    "lane_offset_inside_road_edges",
    "lane_offset_from_y",
  }
  nodes = [node for node in source.body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) and node.name in names]
  assert len(nodes) == len(names)
  namespace = {
    **vars(cluster_config),
    **vars(cluster_models),
    **vars(cluster_utils),
    "dataclass": dataclass,
    "Any": Any,
    "math": math,
    "LANE_CHANGE_MODEL_DIRECT_ONLY": True,
    "ROUTE_REPLAY_USE_LANE_CHANGE_ANIMATION": True,
  }
  exec(compile(ast.Module(nodes, type_ignores=[]), str(path), "exec"), namespace)
  initial = cluster_simulator.ClusterSimulator().update(cluster_models.SimulatorInput(), 0.05)
  defaults = {field.name: getattr(initial, field.name) for field in fields(initial)}
  frame_type = namespace["RouteReplayFrame"]
  values = {field.name: defaults[field.name] for field in fields(frame_type) if field.name in defaults}
  values.update(
    t=0.0,
    lane_center_offset_m=None,
    left_lane_offset=-0.5,
    right_lane_offset=0.5,
    left_lane_visible=True,
    right_lane_visible=True,
    left_lane_style="solid",
    right_lane_style="solid",
    left_lane_color=None,
    right_lane_color=None,
    road_curvature=None,
    road_curvature_source="steeringAngleDeg",
    lane_position_source="default",
    model_lane_lines=(),
    model_road_edges=(),
    model_path_source="none",
    lane_change_source="none",
    lane_change_recenter_start_progress=1.0,
    lane_change_continuation=False,
  )
  return namespace, frame_type(**values), cluster_models.ModelPathPoint


def native_frame(frame, state_function):
  state = asdict(state_function(frame))
  state["lane_width_m"] = frame.lane_width_m
  return {
    "t": frame.t,
    "state": state,
    "lanes": {
      "center": frame.lane_center_offset_m,
      "inner": [frame.left_lane_offset, frame.right_lane_offset],
      "visible": [frame.left_lane_visible, frame.right_lane_visible],
      "extra": [frame.extra_left_lane_visible, frame.extra_right_lane_visible],
      "edges": [frame.left_road_edge_offset, frame.right_road_edge_offset],
      "styles": [frame.left_lane_style, frame.right_lane_style],
      "colors": [frame.left_lane_color, frame.right_lane_color],
      "lines": [[asdict(point) for point in line] for line in frame.model_lane_lines],
      "model_edges": [[asdict(point) for point in line] for line in frame.model_road_edges],
    },
    "metadata": {
      "road_curvature": frame.road_curvature,
      "road_curvature_source": frame.road_curvature_source,
      "lane_position_source": frame.lane_position_source,
      "model_path_source": frame.model_path_source,
      "lane_change_source": frame.lane_change_source,
      "recenter_start": frame.lane_change_recenter_start_progress,
      "continuation": frame.lane_change_continuation,
      "cutin_debug_text": frame.cutin_debug_text,
    },
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source, base, point_type = original(args.root)
  rng = random.Random(25301)
  frames = []
  for index in range(120):
    frame = replace(
      base,
      t=index * 0.05,
      lane_width_m=rng.uniform(2.4, 4.6),
      lane_center_offset_m=None if index % 5 == 0 else rng.uniform(-2, 2),
      lane_change=[None, "left", "right"][index % 3],
      lane_change_phase=["idle", "preparing", "changing", "recentering"][index % 4],
      lane_change_progress=rng.random(),
      lane_change_recenter_start_progress=rng.random(),
      left_signal=index % 2 == 0,
      right_signal=index % 3 == 0,
      left_lane_visible=index % 3 != 0,
      right_lane_visible=index % 4 != 0,
      extra_left_lane_visible=index % 5 != 0,
      extra_right_lane_visible=index % 6 != 0,
      left_lane_offset=rng.uniform(-0.75, -0.25),
      right_lane_offset=rng.uniform(0.25, 0.75),
      left_road_edge_offset=None if index % 4 == 0 else -1.6,
      right_road_edge_offset=None if index % 5 == 0 else 1.6,
      left_lane_style="solid" if index % 2 else "dashed",
      right_lane_style="dashed" if index % 2 else "solid",
      left_lane_color=None if index % 3 else (230, 210, 10),
      right_lane_color=None if index % 4 else (210, 220, 230),
    )
    values = {}
    for field in fields(frame):
      value = getattr(frame, field.name)
      if isinstance(value, float) and field.name not in {
        "t",
        "lane_width_m",
        "left_lane_offset",
        "right_lane_offset",
        "lane_change_progress",
        "lane_change_recenter_start_progress",
      }:
        values[field.name] = rng.uniform(-1, 1)
      elif value is None and field.name.endswith(("_kph", "_mps2", "_m_inv", "_mps", "_rps", "_m", "_ms", "_percent", "_c", "_s", "_mps3")):
        values[field.name] = None if index % 3 == 0 else rng.random()
    for integer in ("speed_limit_kph", "cruise_kph"):
      if values.get(integer) is not None:
        values[integer] = int(values[integer] * 100)
    frame = replace(frame, **values)
    frame = replace(
      frame,
      road_curvature=None if index % 3 == 0 else rng.uniform(-0.02, 0.02),
      radar_points=() if index % 4 == 0 else (source["RadarPoint"](f"F{index}", 10.0 + index, -2.0, "liveTracks", relative_speed_mps=-2.0),),
      detected_vehicles=(source["DetectedVehicle"](f"lead{index}", 20.0 + index, 1.5, cut_in=index % 2 == 0, radar_track_id=index),),
    )
    if index % 4:
      lines = tuple(tuple(point_type(forward_m=float(x), lateral_m=offset + x * 0.01) for x in (0, 10, 30)) for offset in (-5.4, -1.8, 1.8, 5.4))
      frame = replace(frame, model_lane_lines=lines if index % 7 else ((), lines[1], lines[2], ()), model_road_edges=(lines[0], lines[3]))
    frames.append(frame)
  queries, expected = [], []
  for index, left in enumerate(frames):
    right = frames[(index + 1) % len(frames)]
    for amount in (0.0, 0.25, 0.499999, 0.5, 0.75, 1.0):
      blended = source["blend_frames"](left, right, amount)
      queries.append({"left": native_frame(left, source["frame_to_state"]), "right": native_frame(right, source["frame_to_state"]), "amount": amount})
      expected.append(
        {
          "left": asdict(source["frame_to_state"](left)),
          "right": asdict(source["frame_to_state"](right)),
          "blend": asdict(source["frame_to_state"](blended)),
          "t": blended.t,
          "curvature": blended.road_curvature,
        }
      )
  (args.output / "queries.json").write_text(json.dumps(queries))
  (args.output / "source.json").write_text(json.dumps(expected))
  before = hashlib.sha256(args.native.read_bytes()).hexdigest()
  run = subprocess.run([str(args.native.resolve())], input="".join(json.dumps(query) + "\n" for query in queries), capture_output=True, text=True)
  (args.output / "native.jsonl").write_text(run.stdout)
  (args.output / "native.stderr").write_text(run.stderr)
  run.check_returncode()
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  compare(json.loads(json.dumps(expected)), actual)
  assert before == hashlib.sha256(args.native.read_bytes()).hexdigest()
  report = {
    "pairs": len(queries),
    "state_comparisons": len(queries) * 3,
    "native_sha256": before,
    "source_sha256": hashlib.sha256((args.root / "openpilot/selfdrive/carrot/cluster/cluster_route_replay.py").read_bytes()).hexdigest(),
    "float_relative_tolerance": 2e-14,
    "float_absolute_tolerance": 2e-14,
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
