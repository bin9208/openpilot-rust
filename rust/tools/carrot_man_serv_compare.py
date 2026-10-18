import argparse
import ast
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import hashlib
import types
from types import SimpleNamespace as NS
from collections import deque

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).parent))
from carrot_man_compare import compare, snapshot
from openpilot.selfdrive.carrot.navigation_runtime import NavigationRuntime
from openpilot.selfdrive.carrot.navigation_sources import NavigationSource, safety_rejection
from openpilot.common.constants import CV
from openpilot.cereal import log
import numpy as np

def refreshed_source(path):
  fixture=json.loads((ROOT/'rust/upstream/refresh-source.json').read_text())
  assert fixture['source_commit']=='163c000e05641adea88e9221094d45c90c3e186d'
  record=fixture['files'][path];data=record['content'].encode()
  assert hashlib.sha256(data).hexdigest()==record['sha256']
  assert hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==record['git_blob']
  return record['content']


def message(service):
  result = log.Event.new_message()
  result.logMonoTime = 0
  result.valid = False
  result.init(service)
  return result


def source_owner(root, binding, clock):
  sys.modules["openpilot.common.swaglog"] = NS(cloudlog=NS(error=lambda *args, **kwargs: None))
  spec = importlib.util.spec_from_file_location("params_pyx", binding)
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  memory = module.Params(str(root / "memory"))
  params = module.Params(str(root))
  gps=types.ModuleType('verified_gps_sources');sys.modules[gps.__name__]=gps
  exec(compile(refreshed_source('openpilot/selfdrive/carrot/gps_sources.py'),'163c000e:gps_sources','exec'),gps.__dict__)
  control=types.ModuleType('verified_navi_control');sys.modules[control.__name__]=control
  exec(compile(refreshed_source('openpilot/selfdrive/carrot/carrot_navi_control.py'),'163c000e:carrot_navi_control','exec'),control.__dict__)
  # The unchanged source projector constructs this dataclass. Its appended
  # receipt metadata defaults to zero for older senders, as upstream specifies.
  import openpilot.selfdrive.carrot.navigation_runtime as navigation_runtime
  navigation_runtime.NaviVehicleControl=control.NaviVehicleControl
  # Retain this fork's navigation owner and execute upstream's changed methods.
  tree = ast.parse((ROOT/'openpilot/selfdrive/carrot/carrot_serv.py').read_text())
  final_tree=ast.parse(refreshed_source('openpilot/selfdrive/carrot/carrot_serv.py'))
  final_class=next(n for n in final_tree.body if isinstance(n,ast.ClassDef) and n.name=='CarrotServ')
  base_class=next(n for n in tree.body if isinstance(n,ast.ClassDef) and n.name=='CarrotServ')
  replacements={n.name:n for n in final_class.body if isinstance(n,ast.FunctionDef) and n.name in {'_gps_sources','_update_gps','_apply_carrot_navi_vehicle'}}
  base_class.body=[replacements.pop(n.name,n) if isinstance(n,ast.FunctionDef) else n for n in base_class.body]
  base_class.body.extend(replacements.values())
  class PositionFields(ast.NodeTransformer):
    def visit_Assign(self,node):
      if ast.unparse(node.targets[0])=='msg.carrotMan.xPosLat':
        fields=[n for m in final_class.body if isinstance(m,ast.FunctionDef) for n in ast.walk(m) if isinstance(n,ast.Assign) and ast.unparse(n.targets[0]).startswith('msg.carrotMan.position')]
        assert len(fields)==4
        return [node,*fields]
      return node
  PositionFields().visit(base_class)
  init=next(n for n in base_class.body if isinstance(n,ast.FunctionDef) and n.name=='__init__')
  init.body.extend(ast.parse('self.position_mono_time = 0').body)
  clear=next(n for n in base_class.body if isinstance(n,ast.FunctionDef) and n.name=='_clear_carrot_navi_control')
  clear.body.insert(0,ast.parse("self._gps_sources().remove('external_navi')").body[0])
  nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "CarrotServ"]
  nodes += [node for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in {"nav_type_mapping", "COUNTDOWN_NEW_TARGET_MIN_JUMP_M", "SCHOOL_ZONE_GAS_OVERRIDE_TIMEOUT_S"} for target in node.targets)]
  scope = dict(NavigationRuntime=NavigationRuntime, NavigationSource=NavigationSource, Params=lambda path=None: memory if path else params,
    time=NS(monotonic=lambda: clock[0]), collections=NS(deque=deque), np=np, math=math, json=json, CV=CV, safety_rejection=safety_rejection,
    messaging=NS(new_message=lambda name, **kwargs: message(name)), get_gps_location_service=lambda p: "gpsLocationExternal", PC=True, TICI=False, CarrotNaviControl=object,
    GPSSources=gps.GPSSources, print=lambda *args, **kwargs: None)
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), "carrot_serv.py", "exec"), scope)
  return scope["CarrotServ"](), params, memory


def setup_values():
  return {
    "AutoNaviSpeedBumpSpeed": "20", "AutoNaviSpeedBumpTime": "1", "AutoNaviSpeedBumpEndDistance": "300", "AutoNaviSpeedCtrlEnd": "5",
    "AutoNaviRearCameraHoldDistance": "100", "AutoNaviSpeedCtrlMode": "3", "VehicleNaviCanControl": "3", "VehicleNaviSchoolZoneControl": "1",
    "VehicleSpeedCameraControlMode": "2", "AutoNaviSpeedSafetyFactor": "105", "AutoNaviSpeedDecelRate": "120", "AutoNaviCountDownMode": "3",
    "TurnSpeedControlMode": "2", "MapTurnSpeedFactor": "100", "AutoTurnControlSpeedTurn": "25", "AutoTurnMapChange": "1", "AutoTurnControl": "2",
    "AutoTurnControlTurnEnd": "5", "AutoCurveSpeedLowerLimit": "30", "IsMetric": "1", "AutoRoadSpeedLimitOffset": "0", "AutoCurveSpeedFactor": "100", "LanguageSetting": "main_ko",
  }


def scenarios(root):
  requests = [dict(op="configure", root=str(root), values=setup_values())]
  car = dict(vEgo=20., vEgoCluster=20., aEgo=0., vCluRatio=1., speedLimit=50., speedLimitDistance=100., speedBumpDistance=0., gasPressed=False, brakePressed=False)
  for index in range(24):
    now = 100. + index * 0.5
    current_car = {**car}
    if index in (2, 3, 7, 8, 9):
      current_car["gasPressed"] = True
    if index == 4:
      current_car["brakePressed"] = True
    if 6 <= index <= 13:
      current_car["schoolZoneActive"] = True
    if index in (15, 16):
      current_car["vehicleNaviSectionActive"] = True
      current_car["vehicleNaviSpeed"] = 40.
    if index == 14:
      current_car["speedBumpDistance"] = 20.
    nav = None
    if index in (0, 1, 17, 18, 19, 20, 21, 22):
      kind = 75 if index in (17, 18) else 22 if index in (19, 20) else 1
      nav = dict(source="naver_v1", session_id="navi-a" if index < 17 else "navi-b", sequence=index + 1, lifecycle="guiding", received_mono_s=now,
        control=dict(road_limit_kph=60., road_category=2, safety=dict(type=kind, distance_m=40. if kind == 75 else 80., speed_limit_kph=40., received_mono_s=now),
          current=dict(present=True, turn_type=12 if index < 17 else 7, distance_m=100., next_road_width=5),
          position_present=True, position_latitude=37., position_longitude=127., position_heading_deg=45., position_received_mono_s=now,
          traffic_present=index in (0, 17), traffic_visible=True, traffic_distance_m=70., traffic_source="test", traffic_lamp="red", traffic_remain_s=20))
      if index == 22:
        nav["lifecycle"] = "arrived"
        nav["control"] = {}
    gps = dict(car_updated=index in (0, 1, 17, 18), control_updated=index in (0, 1, 17, 18), gps_updated=False, has_fix=False, bearing_deg=0., latitude=0., longitude=0.)
    requests.append(dict(op="tick", input=dict(now=now, car=current_car if index != 23 else None, selfdrive_alive=index != 23,
      distance_traveled=index * 10., vision_speed=45. if index in (11, 12) else 250., route_speed=40. if index in (12, 13) else 300., gps=gps), snapshot=nav, v2=None, legacy=None, refresh=True))
  return requests


def car_reader(raw):
  value = message("carState").carState
  fields = {key: val for key, val in raw.items() if key in value.schema.fields}
  value.from_dict(fields)
  return value


def decode(raw, service):
  with log.Event.from_bytes(bytes(raw)) as event:
    return dict(valid=event.valid, payload=getattr(event, service).to_dict())


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", required=True)
  parser.add_argument("--params-binding", required=True)
  parser.add_argument("--evidence", required=True, type=Path)
  args = parser.parse_args()
  os.environ["OPENPILOT_PREFIX"] = "d"
  with tempfile.TemporaryDirectory(prefix="carrot-man-219-") as temporary:
    root = Path(temporary)
    requests = scenarios(root)
    result = subprocess.run([args.binary], input="".join(json.dumps(r) + "\n" for r in requests), capture_output=True, text=True)
    if result.returncode:
      raise RuntimeError(result.stderr)
    actual = [json.loads(line) for line in result.stdout.splitlines()]
    clock = [100.]
    owner, params, memory = source_owner(root, args.params_binding, clock)
    expected = []
    for request, rust in zip(requests[1:], actual[1:]):
      i = request["input"]
      clock[0] = i["now"]
      if request["snapshot"]:
        owner.navigation_runtime.accept_snapshot(snapshot(request["snapshot"]))
      owner.navigation_selection, navi = owner.navigation_runtime.select(i["now"])
      selected = owner.navigation_selection.snapshot
      owner.nTBTNextRoadWidth = selected.control.current.next_road_width if selected else 0
      owner._project_carrot_navi_control(navi)
      captured = []
      pub = NS(send=lambda name, event: captured.append((name, dict(valid=event.valid, payload=getattr(event, name).to_dict()))))
      car = car_reader(i["car"]) if i["car"] else message("carState").carState
      sm = {"carState": car, "selfdriveState": NS(distanceTraveled=i["distance_traveled"]), "carControl": message("carControl").carControl,
        "gpsLocationExternal": message("gpsLocationExternal").gpsLocationExternal, "navInstruction": message("navInstruction").navInstruction}
      class Frame(dict):
        pass
      sm = Frame(sm)
      sm.alive = {"carState": i["car"] is not None, "selfdriveState": i["selfdrive_alive"], "navInstruction": False, "gpsLocationExternal": False}
      sm.valid = {"navInstruction": False, "carState": i["car"] is not None, "gpsLocationExternal": False}
      sm.recv_time = {"gpsLocationExternal": 0.}
      sm.updated = {"carState": i["gps"]["car_updated"], "carControl": i["gps"]["control_updated"], "gpsLocationExternal": i["gps"]["gps_updated"]}
      owner.update_navi("127.0.0.1", sm, pub, i["vision_speed"], [], [], i["route_speed"], "gpsLocationExternal", navigation_prepared=True)
      source = dict(captured)
      rust_publications = {"carrotMan": decode(rust["carrot"], "carrotMan"), "navInstructionCarrot": decode(rust["instruction"], "navInstructionCarrot")}
      compare(rust_publications, source, f"tick@{i['now']}")
      expected_ts = owner.navigation_selection.snapshot.control.traffic_received_mono_s if owner.carrot_navi_traffic_active else None
      deadline = time.monotonic() + 2.
      while True:
        traffic_value = owner.params_memory.get("TrafficLight")
        expected_traffic = json.loads(traffic_value) if traffic_value else None
        if (expected_ts is None and expected_traffic is None) or (expected_traffic and expected_traffic.get("ts") == expected_ts):
          break
        if time.monotonic() >= deadline:
          raise AssertionError("original asynchronous Params write did not complete")
        time.sleep(0.005)
      compare(rust["traffic"], expected_traffic, f"TrafficLight@{i['now']}")
      expected.append(dict(now=i["now"], publications=source, traffic=expected_traffic))
    args.evidence.mkdir(parents=True, exist_ok=True)
    (args.evidence / "serv-inputs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in requests))
    (args.evidence / "serv-rust.jsonl").write_text(result.stdout)
    (args.evidence / "serv-source.jsonl").write_text("".join(json.dumps(r) + "\n" for r in expected))
    (args.evidence / "serv-summary.json").write_text(json.dumps(dict(passed=True, scenarios=len(expected), surface="original cereal publications and real isolated native Params"), indent=2) + "\n")
    print(f"PASS {len(expected)} original CarrotServ cereal publication scenarios")


if __name__ == "__main__":
  main()
