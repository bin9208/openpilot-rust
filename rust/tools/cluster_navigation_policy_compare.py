from __future__ import annotations

import ast
from dataclasses import asdict, replace
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys


def run(root: Path, binary: Path, output: Path) -> None:
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_models as models
  from cluster_model_types import default_record

  source = root / "openpilot/selfdrive/carrot/cluster/cluster_renderer.py"
  names = {"_navi_live_panel_visible", "_navi_map_frame_present", "_navi_signal_active", "_navi_primary_signal_seconds"}
  tree = ast.parse(source.read_text())
  renderer = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "ClusterUiRenderer")
  renderer.body = [node for node in renderer.body if isinstance(node, ast.FunctionDef) and node.name in names]
  scope = dict(vars(models))
  exec(compile(ast.Module(body=[renderer], type_ignores=[]), str(source), "exec"), scope)
  reference = scope[renderer.name]()
  output.mkdir(parents=True, exist_ok=True)
  cases, expected = [], []
  signal_names = ["red", "left", "uturn", "straight", "right"]

  def case(navi=None, dashboard=None, traffic=None, remaining=(None,) * 5):
    cases.append(
      {
        "navi": asdict(navi) if navi else None,
        "dashboard": asdict(dashboard) if dashboard else None,
        "traffic": asdict(traffic) if traffic else None,
        "remaining": remaining,
      }
    )
    expected.append(
      {
        "visible": reference._navi_live_panel_visible(navi),
        "map": reference._navi_map_frame_present(dashboard),
        "active": [reference._navi_signal_active(traffic, name, seconds) for name, seconds in zip(signal_names, remaining, strict=True)],
        "primary": reference._navi_primary_signal_seconds(traffic, **{name + "_s": seconds for name, seconds in zip(signal_names, remaining, strict=True)}),
      }
    )

  case()
  base = default_record(models.NaviLiveState)
  case(navi=base)
  for name, cls in [
    ("vehicle", models.NaviVehicleInfo),
    ("current", models.NaviGuidanceInfo),
    ("next", models.NaviGuidanceInfo),
    ("lane_current", models.NaviLaneInfo),
    ("route", models.NaviRouteInfo),
    ("traffic_light", models.NaviTrafficLightInfo),
  ]:
    case(navi=replace(base, **{name: default_record(cls)}))
  case(navi=replace(base, lane_ahead=(default_record(models.NaviLaneInfo),)))
  for visible in [False, True]:
    case(navi=replace(base, crossroad=replace(default_record(models.NaviCrossroadInfo), visible=visible)))
    case(navi=replace(base, status=replace(default_record(models.NaviStatusInfo), guidance_active=visible)))
  speed = default_record(models.NaviSpeedInfo)
  for primary, secondary, section in itertools.product([None, 0, 3], [None, 3], [False, True]):
    case(navi=replace(base, speed=replace(speed, sdi_type=primary, secondary_sdi_type=secondary, section_active=section)))
  for connected, stalled, present, key in itertools.product([False, True], [False, True], [False, True], ["render:map_main", "image:other"]):
    frame = replace(default_record(models.NaviMediaFrame), key=key, present=present)
    dashboard = replace(default_record(models.NaviDashboardState), connected=connected, map_stream_stalled=stalled, media=(frame,))
    case(dashboard=dashboard)
  traffic = default_record(models.NaviTrafficLightInfo)
  for flags in itertools.product([None, False, True], repeat=5):
    values = {name + "_on": flag for name, flag in zip(signal_names, flags, strict=True)}
    values.update({name + "_s": seconds for name, seconds in zip(signal_names, [12, 3, None, 5, 0], strict=True)})
    current = replace(traffic, **values)
    for remaining in [(None,) * 5, (0,) * 5, (None, 2, 4, None, 10)]:
      case(traffic=current, remaining=remaining)
  input_path = output / "input.json"
  input_path.write_text(json.dumps(cases))
  native = json.loads(subprocess.check_output([str(binary), str(input_path)], text=True))
  expected = json.loads(json.dumps(expected))
  (output / "expected.json").write_text(json.dumps(expected, indent=2))
  (output / "native.json").write_text(json.dumps(native, indent=2))
  failures = [index for index, (actual, reference) in enumerate(zip(native, expected, strict=True)) if actual != reference]
  result = {
    "cases": len(cases),
    "failures": failures,
    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "methods": sorted(names),
  }
  (output / "result.json").write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  if failures:
    raise SystemExit(1)


if __name__ == "__main__":
  run(*(Path(arg).resolve() for arg in sys.argv[1:]))
