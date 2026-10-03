#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# ─── How to run ───
# Use the repository's verified oracle environment; do not install dependencies:
# python rust/tools/check_card_cruise.py --binary PATH --evidence DIR
# ──────────────────
from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
from typing import Final

import numpy as np
from openpilot.cereal import car, messaging
from openpilot.common.constants import CV
from openpilot.selfdrive.carrot.bluetooth import model as bluetooth
from openpilot.selfdrive.carrot.carrot_man_input import get_carrot_man
from openpilot.selfdrive.carrot.cruise_gap import cruise_gap_levels, next_gap_personality, supported_gap_levels
import openpilot.selfdrive.carrot.carrot_man_input as navigation

ROOT: Final = Path(__file__).resolve().parents[2]
SOURCE: Final = ROOT / "openpilot/selfdrive/car/cruise.py"
DEFAULTS: Final = {
  "AutoEngage": "0", "UseLaneLineSpeed": "0", "CruiseSpeedUnit": "10", "CruiseSpeedUnitBasic": "1",
  "CruiseButtonLongDelay": "40", "CruiseButtonMode": "0", "AutoCruiseControl": "1", "AutoGasTokSpeed": "20",
  "AutoGasCancelSpeed": "30", "AutoGasSyncSpeed": "1", "ApplyModelSpeed": "0", "AutoSpeedUptoRoadSpeedLimit": "0",
  "AutoRoadSpeedAdjust": "0", "SpeedFromPCM": "0", "PaddleMode": "2", "CancelButtonMode": "0", "LfaButtonMode": "0",
  "AutoRoadSpeedLimitOffset": "0", "AutoNaviSpeedSafetyFactor": "110", "CruiseOnDist": "300",
  "CruiseSpeed1": "0", "CruiseSpeed2": "50", "CruiseSpeed3": "70", "CruiseSpeed4": "90", "CruiseSpeed5": "110",
  "LongitudinalPersonalityMax": "4", "CruiseGapLevels": "4", "LongitudinalPersonality": "3", "MyDrivingMode": "1",
  "SoftHoldOnCancel": "0", "DisengageOnAccelerator": "0", "ActivateCruiseAfterBrake": "0",
}


class OracleParams:
  """Mutable in-memory Params I/O boundary; original cruise method bodies are unchanged."""
  def __init__(self, values: dict[str, str]) -> None:
    self.values = values
    self.writes: list[list[str]] = []

  def get_int(self, key: str) -> int:
    return int(self.values.get(key, "0"))

  def get_float(self, key: str) -> float:
    return float(self.values.get(key, "0"))

  def get_bool(self, key: str) -> bool:
    return self.values.get(key) == "1"

  def put_int_nonblocking(self, key: str, value: int) -> None:
    self.values[key] = str(value)
    self.writes.append([key, str(value)])

  def put_bool_nonblocking(self, key: str, value: bool) -> None:
    self.values[key] = "1" if value else "0"
    self.writes.append([key, self.values[key]])


def helper_type(params: OracleParams, command_root: Path):
  """Compile unchanged source AST, replacing only external Params construction."""
  parsed = ast.parse(SOURCE.read_text())
  selected = [node for node in parsed.body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) and node.name in ("VCruiseCarrot", "is_hold_interlock_active")]
  namespace = dict(math=__import__("math"), np=np, car=car, CV=CV, get_carrot_man=get_carrot_man,
    cruise_gap_levels=cruise_gap_levels, next_gap_personality=next_gap_personality, supported_gap_levels=supported_gap_levels,
    BLUETOOTH_CANCEL=bluetooth.BLUETOOTH_CANCEL, REMOTE_BUTTONS=bluetooth.REMOTE_BUTTONS,
    CommandReader=lambda channel: bluetooth.CommandReader(channel, command_root), Params=lambda *args: params,
    GearShifter=car.CarState.GearShifter, ButtonType=car.CarState.ButtonEvent.Type, ButtonEvent=car.CarState.ButtonEvent,
    V_CRUISE_UNSET=255, AUTO_CRUISE_MAX_STEERING_ANGLE=70.0)
  exec(compile(ast.Module(body=selected, type_ignores=[]), str(SOURCE), "exec"), namespace)
  return namespace["VCruiseCarrot"]


def frame(**fields):
  cs = car.CarState.new_message(vEgo=80/3.6, vEgoCluster=80/3.6, canValid=True, gearShifter="drive",
    cruiseState=dict(available=True), engineRpm=1234, leftBlindspot=True)
  cs.from_dict(fields)
  return dict(cs=list(cs.to_bytes()), events=[], metric=True, advance=.031, params={}, files={})


def fixtures():
  cases = []
  for metric in (True, False):
    for pcm in (False, True):
      for mode in range(4):
        steps = [frame(), frame(buttonEvents=[dict(type="accelCruise", pressed=True)]), frame(buttonEvents=[dict(type="accelCruise", pressed=False)])]
        for step in steps:
          step["metric"] = metric
          cc = messaging.new_message("carControl"); cc.carControl.enabled = True
          step["events"] = [list(cc.to_bytes())]
        cases.append(dict(name=f"short-accel-{metric}-{pcm}-{mode}", cp=dict(pcmCruise=pcm, openpilotLongitudinalControl=not pcm), params=DEFAULTS | {"CruiseButtonMode":str(mode)}, frames=steps))
  from card_qa.cruise.scenarios import scenarios
  return cases + scenarios()


def oracle(case):
  with tempfile.TemporaryDirectory(prefix="card-cruise-source-") as root:
    clock = SimpleNamespace(now=10.)
    bluetooth.time.monotonic = lambda: clock.now
    navigation.time.monotonic = lambda: clock.now
    params = OracleParams(dict(case["params"]))
    helper = helper_type(params, Path(root))(car.CarParams.new_message(**case["cp"]))
    sm = SimpleNamespace(alive={}, seen={}, valid={}, recv_time={})
    services = ["carControl", "carrotMan", "longitudinalPlan", "radarState", "drivingModelData"]
    data = {name:getattr(messaging.new_message(name), name) for name in services}
    for name in services:
      sm.alive[name]=False; sm.seen[name]=False; sm.valid[name]=False; sm.recv_time[name]=0.
    class Subscriptions(SimpleNamespace):
      def __getitem__(self, name):
        return data[name]
    sm = Subscriptions(**vars(sm))
    result=[]; previous=car.CarState.new_message(); enabled_last=False
    for step in case["frames"]:
      clock.now += step["advance"]
      params.values.update(step["params"])
      for name, value in step["files"].items():
        (Path(root)/name).write_text(json.dumps(value))
      for payload in step["events"]:
        with car.CarState.from_bytes(bytes(step["cs"])) as _:
          pass
        with messaging.log.Event.from_bytes(bytes(payload)) as event:
          name=event.which(); data[name]=getattr(event, name).as_builder()
          sm.alive[name]=True; sm.seen[name]=True; sm.valid[name]=event.valid; sm.recv_time[name]=clock.now
      for name in step.get("dead", []): sm.alive[name]=False
      with car.CarState.from_bytes(bytes(step["cs"])) as reader:
        cs=reader.as_builder()
      prints=io.StringIO()
      with contextlib.redirect_stdout(prints):
        helper.update_v_cruise(cs,sm,step["metric"])
        if data["carControl"].enabled and not enabled_last: helper.initialize_v_cruise(previous,False)
      cs.logCarrot=helper.log; cs.vCruise=float(0 if helper._paddle_decel_active else helper.v_cruise_kph)
      cs.vCruiseCluster=float(0 if helper._paddle_decel_active else helper.v_cruise_cluster_kph)
      cs.softHoldActive=helper._soft_hold_active; cs.activateCruise=helper._activate_cruise
      cs.latEnabled=helper._lat_enabled; cs.useLaneLineSpeed=helper.useLaneLineSpeedApply; cs.carrotCruise=int(helper.carrot_cruise_active)
      snapshot={key:value for key,value in vars(helper).items() if key not in ("CP","params","params_memory","bluetooth_commands")}
      snapshot["button_prev"]=int(snapshot["button_prev"].raw) if hasattr(snapshot["button_prev"],"raw") else int(snapshot["button_prev"])
      snapshot=json.loads(json.dumps(snapshot,default=lambda value:value.item()))
      result.append(dict(cs=cs.to_dict(), state=snapshot, writes=params.writes[:], prints=prints.getvalue().splitlines()))
      params.writes.clear(); previous=cs; enabled_last=data["carControl"].enabled
    return result


def main() -> None:
  parser=argparse.ArgumentParser(); parser.add_argument("--binary",type=Path); parser.add_argument("--evidence",type=Path,required=True)
  parser.add_argument("--red-identity",action="store_true"); parser.add_argument("--scenario-prefix",default=""); args=parser.parse_args()
  args.evidence.mkdir(parents=True,exist_ok=True)
  (args.evidence/"result.json").unlink(missing_ok=True)
  request=[case for case in fixtures() if case["name"].startswith(args.scenario_prefix)]; expected=[oracle(case) for case in request]
  assert request, "No scenarios selected"
  (args.evidence/"request.json").write_text(json.dumps(request)+"\n")
  (args.evidence/"source.json").write_text(json.dumps(expected)+"\n")
  if args.red_identity:
    with car.CarState.from_bytes(bytes(request[0]["frames"][0]["cs"])) as cs: actual=cs.to_dict()
    assert actual == expected[0][0]["cs"], f"Missing cruise tail: identity vCruise={actual.get('vCruise')} expected={expected[0][0]['cs']['vCruise']}"
  output=args.evidence/"native.json"
  output.unlink(missing_ok=True)
  child=subprocess.run([args.binary.resolve(),output.resolve()],input=json.dumps(request),text=True,capture_output=True,check=False)
  (args.evidence/"process.log").write_text(child.stdout+child.stderr+f"\nEXIT {child.returncode}\n"); child.check_returncode()
  actual=json.loads(output.read_text())
  for case in actual:
    for step in case:
      with car.CarState.from_bytes(bytes(step["cs"])) as cs: step["cs"]=cs.to_dict()
  (args.evidence/"native-fields.json").write_text(json.dumps(actual)+"\n")
  if expected != actual:
    for i,(source,native) in enumerate(zip(expected,actual,strict=True)):
      if source != native:
        (args.evidence/"failure.json").write_text(json.dumps(dict(case=request[i]["name"],source=source,native=native),indent=2)+"\n")
        raise AssertionError(f"Cruise mismatch {request[i]['name']}")
  if not args.scenario_prefix:
    commands=[step for case in expected for step in case if any(line.startswith("Carrot command(cruise.py):") for line in step["prints"])]
    assert commands, "No valid Carrot command reached the original source"
    assert any(step["state"]["nRoadLimitSpeed"]>0 for case in expected for step in case), "No valid navigation limit reached the original source"
  report=dict(status="pass",cases=len(request),frames=sum(len(case["frames"]) for case in request),source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),runtime_python=False,observable="full CarState wire fields, temporal helper state, Params writes, print effects")
  (args.evidence/"result.json").write_text(json.dumps(report,indent=2)+"\n"); print(json.dumps(report))


if __name__ == "__main__": main()
