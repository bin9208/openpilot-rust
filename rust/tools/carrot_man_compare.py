import argparse
import ast
import dataclasses
import json
import math
from pathlib import Path
import subprocess
import sys
from collections import deque
from types import SimpleNamespace as NS

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from openpilot.selfdrive.carrot.curve_speed import CurveSpeed, VisionCurveSpeed, curve_speed
from openpilot.selfdrive.carrot.navigation_sources import (
  NavigationControlState, NavigationInstruction, NavigationLifecycle,
  NavigationSnapshot, NavigationSource, NavigationSourceStore, SafetyItem, choose_safety,
)
import numpy as np
from shapely.geometry import LineString


def source_geometry():
  tree = ast.parse((ROOT / "openpilot/selfdrive/carrot/carrot_man.py").read_text())
  names = {"limit_route_points", "haversine", "closest_point_on_segment", "get_path_after_distance", "gps_to_relative_xy", "calculate_curvature"}
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
  owner = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "CarrotMan")
  owner.body = [node for node in owner.body if isinstance(node, ast.FunctionDef) and node.name in {"_update_carrot_navi_route", "carrot_navi_route", "send_routes"}]
  nodes.append(owner)
  scope = {"math": math, "np": np, "LineString": LineString, "SHAPELY_AVAILABLE": True, "NAVI_ROUTE_MAX_POINTS": 4096,
    "V_CURVE_LOOKUP_BP": [0., 1/800, 1/670, 1/560, 1/440, 1/360, 1/265, 1/190, 1/135, 1/85, 1/55, 1/30, 1/25],
    "V_CRUVE_LOOKUP_VALS": [300, 150, 120, 110, 100, 90, 80, 70, 60, 50, 40, 15, 5],
    "messaging": NS(new_message=lambda *args, **kwargs: NS(navRoute=NS())), "print": lambda *args: None}
  prefix = ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0)
  exec(compile(ast.fix_missing_locations(ast.Module(body=[prefix, *nodes], type_ignores=[])), "carrot_man.py", "exec"), scope)
  owner = scope["CarrotMan"]()
  owner.navi_points = []
  owner.navi_points_start_index = 0
  owner.navi_points_active = owner.navd_active = owner.carrot_navi_route_owned = False
  owner.carrot_navi_route_session_id = ""
  owner.carrot_navi_route_sequence = -1
  owner.active_carrot_last = 0
  owner.carrot_serv = NS()
  owner.pm = NS(send=lambda *args: None)
  return scope, owner


def snapshot(raw):
  c = dict(raw["control"])
  for key in ("current", "next"):
    if key in c:
      c[key] = NavigationInstruction(**c[key])
  for key in ("safety", "secondary_safety"):
    if c.get(key) is not None:
      c[key] = SafetyItem(**c[key])
  for key in ("route_points",):
    if key in c:
      c[key] = tuple(tuple(p) for p in c[key])
  if c.get("destination") is not None:
    c["destination"] = tuple(c["destination"])
  return NavigationSnapshot(**{**raw, "source": NavigationSource(raw["source"]), "lifecycle": NavigationLifecycle(raw["lifecycle"]), "activation_epoch": raw.get("activation_epoch", 0), "control": NavigationControlState(**c)})


def compare(actual, expected, path="root"):
  if isinstance(expected, dict):
    assert actual.keys() == expected.keys(), (path, actual.keys(), expected.keys())
    for key in expected:
      compare(actual[key], expected[key], f"{path}.{key}")
  elif isinstance(expected, list):
    assert len(actual) == len(expected), (path, len(actual), len(expected))
    for i, (a, e) in enumerate(zip(actual, expected)):
      compare(a, e, f"{path}[{i}]")
  elif isinstance(expected, float):
    assert math.isclose(actual, expected, rel_tol=1e-10, abs_tol=1e-8), (path, actual, expected)
  else:
    assert actual == expected, (path, actual, expected)


def cases():
  model = {"x": [i * 4. for i in range(33)], "y": [0.] * 33, "z": [0.] * 33, "velocity": [20.] * 33, "yaw_rate": [0.4] * 33}
  curve_input = dict(v_ego=20., sensitivity=1., lower_limit_kph=30., speed_ratio=0.9, a_ego=0.5)
  requests = [dict(op="curve", model=model, input=curve_input, now=1., model_time=1.)]
  for i, value in enumerate([50., 60., 70., 80.]):
    requests.append(dict(op="curve_update", result=dict(approach_kph=value, curve_kph=value, distance=30., direction=1.), now=1. + i * 0.1, model_time=1. + i * 0.1))
  for now in [1.5, 1.7, 1.9, 2.1]:
    requests.append(dict(op="curve_update", result=None, now=now, model_time=None))
  for velocity in [2., 0., 20.]:
    changed = {**model, "velocity": [velocity] * 33, "yaw_rate": [0.] * 33}
    requests.append(dict(op="curve", model=changed, input=curve_input, now=3. + velocity, model_time=3. + velocity))
  points = [(127. + i * 0.00003, 37. + i * 0.000025) for i in range(200)]
  requests += [dict(op="geometry", start=cursor, points=points, position=(127.0001, 37.0001), distance=300., heading=50.) for cursor in [0, 5, 190]]
  ri = dict(onroad=True, active_carrot=2, position=(127., 37.), heading_deg=30., road_limit=50., deceleration=1.2, v_ego=20.)
  update = dict(session_id="v2", sequence=1, present=True, polyline=[(p[1], p[0]) for p in points], force=False, onroad=True)
  requests += [dict(op="route", update=update, input=ri, geos=True), dict(op="route", update=update, input=ri, geos=False),
    dict(op="route", update=update, input=ri, geos=True), dict(op="route", update=None, input=ri, geos=True)]
  policy = dict(mode=3, safety_factor=1.05, bump_speed_kph=20.)
  for i, (source, now, session, seq, state) in enumerate([
    ("tmap_legacy", 10., "t1", 1, "guiding"), ("naver_v1", 10., "n1", 1, "guiding"),
    ("carrot_navi_v2", 10.1, "v1", 1, "guiding"), ("naver_v1", 10.2, "n1", 2, "guiding"),
    ("naver_v1", 10.3, "n2", 1, "guiding"), ("naver_v1", 10.4, "n2", 1, "guiding"),
    ("naver_v1", 12.5, "n2", 2, "guiding"), ("carrot_navi_v2", 12.6, "v1", 2, "stopped"),
    ("naver_v1", 12.7, "n2", 3, "arrived"), ("naver_v1", 12.8, "n2", 4, "guiding"),
  ]):
    c = dict(road_limit_kph=60., road_category=2, safety=dict(type=22 if i == 4 else 1, distance_m=200., speed_limit_kph=60., received_mono_s=now), current=dict(present=True, turn_type=12, distance_m=100.))
    requests.append(dict(op="sources", snapshot=dict(source=source, session_id=session, sequence=seq, lifecycle=state, received_mono_s=now, control=c), now=now, loss=None, policy=policy, hda=(50., 100.)))
  requests += [dict(op="sources", snapshot=None, now=now, loss=loss, policy=policy, hda=(50., 100.)) for now, loss in [(13., ("tmap_legacy", "t1")), (14., None), (23., None)]]
  return requests


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", required=True)
  parser.add_argument("--evidence", required=True, type=Path)
  args = parser.parse_args()
  requests = cases()
  result = subprocess.run([args.binary], input="".join(json.dumps(r) + "\n" for r in requests), text=True, capture_output=True)
  if result.returncode:
    raise RuntimeError(result.stderr)
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  vision = VisionCurveSpeed()
  store = NavigationSourceStore()
  scope, owner = source_geometry()
  expected = []
  for r in requests:
    if r["op"] in ("curve", "curve_update"):
      value = CurveSpeed(**r["result"]) if r["op"] == "curve_update" and r["result"] else None
      if r["op"] == "curve":
        m = r["model"]
        value = curve_speed(NS(position=NS(x=m["x"], y=m["y"], z=m["z"]), velocity=NS(x=m["velocity"]), orientationRate=NS(z=m["yaw_rate"])), **r["input"])
      speed = vision.update(value, r["now"], model_time=r["model_time"])
      output = dict(speed=speed, state={**vision.__dict__, "geometry": list(vision.geometry)})
      if r["op"] == "curve":
        output["result"] = None if value is None else dataclasses.asdict(value)
    elif r["op"] == "geometry":
      path, index, closest = scope["get_path_after_distance"](r["start"], r["points"], r["position"], r["distance"])
      output = dict(path=dict(points=path, start_index=index, closest=closest), relative=None if closest is None else scope["gps_to_relative_xy"](path, closest, r["heading"]))
    elif r["op"] == "route":
      ri = r["input"]
      owner.params = NS(get_bool=lambda key: ri["onroad"])
      u = r["update"]
      coordinates = []
      owner.pm = NS(send=lambda channel, message: coordinates.append(message.navRoute.coordinates))
      navi = None if u is None else NS(session_id=u["session_id"], route=NS(sequence=u["sequence"], present=u["present"], polyline=u["polyline"]))
      owner._update_carrot_navi_route(navi, force=False if u is None else u["force"])
      owner.carrot_serv = NS(active_carrot=ri["active_carrot"], vpPosPointLon=ri["position"][0], vpPosPointLat=ri["position"][1], bearing=ri["heading_deg"], nRoadLimitSpeed=ri["road_limit"], autoNaviSpeedDecelRate=ri["deceleration"], autoNaviSpeedCtrlEnd=5.)
      owner.sm = {"carState": NS(vEgo=ri["v_ego"])}
      scope["SHAPELY_AVAILABLE"] = r["geos"]
      points, distances, speed = owner.carrot_navi_route()
      output = dict(coordinates=coordinates[0] if coordinates else None, output=dict(points=points, distances=distances, speed=speed), state=dict(points=owner.navi_points, start_index=owner.navi_points_start_index, active=owner.navi_points_active, navd_active=owner.navd_active, last_active_carrot=owner.active_carrot_last, session_id=owner.carrot_navi_route_session_id, sequence=owner.carrot_navi_route_sequence, owned=owner.carrot_navi_route_owned))
    else:
      accepted = None if r["snapshot"] is None else store.accept(snapshot(r["snapshot"]), r["now"])
      lost = None if r["loss"] is None else store.record_transport_loss(NavigationSource(r["loss"][0]), r["loss"][1], r["now"])
      selection = store.select(r["now"])
      safety = choose_safety(selection, *r["hda"], **r["policy"])
      output = dict(accepted=accepted, lost=lost, selection=dataclasses.asdict(selection), safety=None if safety is None else dataclasses.asdict(safety))
    expected.append(json.loads(json.dumps(output)))
  assert len(actual) == len(expected)
  for index, (a, e) in enumerate(zip(actual, expected)):
    compare(a, e, f"scenario[{index}] {requests[index]['op']}")
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / "helper-inputs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in requests))
  (args.evidence / "helper-source.jsonl").write_text("".join(json.dumps(r) + "\n" for r in expected))
  (args.evidence / "helper-rust.jsonl").write_text(result.stdout)
  (args.evidence / "helper-summary.json").write_text(json.dumps(dict(passed=True, scenarios=len(requests), categories=["curve", "geometry", "route-active-geos", "route-optional-fallback", "source-arbitration"]), indent=2) + "\n")
  print(f"PASS {len(requests)} source/output/state scenarios")


if __name__ == "__main__":
  main()
