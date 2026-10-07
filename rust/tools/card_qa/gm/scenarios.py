from __future__ import annotations

import itertools
import random
from can_source import load


def cases(op: str | None = None):
  load()
  from opendbc.can import CANPacker
  from opendbc.car.gm.values import CAR
  from openpilot.cereal import car

  result = []
  for candidate in CAR:
    for profile, (alpha, pedal, ev, torque) in enumerate(itertools.product((False, True), repeat=4)):
      pt = [[0x201, 6]] if pedal else []
      if profile % 3:
        pt.append([0xbe, 8])
      if profile % 2:
        pt.extend([[608, 8], [0x142, 8]])
      camera = [[0x320, 8]] if profile % 4 else []
      result.append({"name": f"params-{candidate}-{profile}", "op": "params", "candidate": str(candidate),
        "alpha_long": alpha, "fingerprints": [[0, pt], [2, camera]], "firmware": [],
        "settings": {"EVTable": str(int(ev)), "LateralTorqueCustom": str(int(torque)), "LongActuatorDelay": ("0", "50.5", "99.25")[profile % 3],
          "NNFF": str(profile % 2), "NNFFLite": str((profile // 2) % 2), "DisableMinSteerSpeed": str((profile // 3) % 2)},
        "now": 2_000_000_000, "steps": []})
  if op == "params":
    return result
  CANPacker("gm_global_a_object")
  CANPacker("gm_global_a_chassis")
  rng = random.Random(177131)
  for profile, (candidate, alpha) in enumerate(itertools.product((c for c in CAR if str(c) != "GMC_YUKON_CC"), (False, True))):
    packer = CANPacker("gm_global_a_powertrain_volt")
    steps = []
    for index in range(360):
      now = 2_000_000_000 + index * 10_000_000
      speed = (0., 0.04, 0.1, 0.4, 1.3, 1.6, 2.5, 5.5, 10., 20., 30.)[(index // 7) % 11]
      messages = {
        "ASCMSteeringButton": {"ACCButtons": (0, 1, 2, 3, 5, 6, 7)[(index // 3) % 7], "DistanceButton": (index // 7) % 2, "LKAButton": (index // 11) % 2, "RollingCounter": index % 4},
        "BCMBlindSpotMonitor": {"LeftBSM": index % 4, "RightBSM": (index // 2) % 4},
        "EBCMWheelSpdRear": {"RLWheelSpd": speed * 3.6, "RRWheelSpd": speed * 3.6 + 0.01, "RLWheelDir": (index // 13) % 4, "RRWheelDir": (index // 17) % 4},
        "EBCMWheelSpdFront": {"FLWheelSpd": speed * 3.6 + 0.02, "FRWheelSpd": speed * 3.6},
        "ECMPRDNL2": {"ManualMode": (index // 19) % 2, "PRNDL2": (index // 5) % 16},
        "EBCMBrakePedalPosition": {"BrakePedalPosition": (0, 1, 208, 255)[(index // 9) % 4]},
        "ECMAcceleratorPos": {"BrakePedalPos": (0, 9, 10, 11, 200)[(index // 8) % 5]},
        "ECMEngineStatus": {"BrakePressed": index % 2, "CruiseMainOn": (index // 11) % 2},
        "EBCMRegenPaddle": {"RegenPaddle": (index // 7) % 2},
        "EVDriveMode": {"SinglePedalModeActive": (index // 5) % 2},
        "AcceleratorPedal2": {"AcceleratorPedal2": (0, 1, 127, 254)[(index // 8) % 4], "CruiseState": (0, 1, 4, 3)[(index // 11) % 4]},
        "PSCMSteeringAngle": {"SteeringWheelAngle": index % 51 - 25, "SteeringWheelRate": index % 33 - 16},
        "BCMDoorBeltStatus": {"FrontLeftDoor": index % 2, "FrontRightDoor": (index // 2) % 2, "RearLeftDoor": (index // 3) % 2, "RearRightDoor": (index // 4) % 2, "LeftSeatBelt": (index // 9) % 2},
        "BCMTurnSignals": {"TurnSignals": (index // 5) % 4},
        "BCMGeneralPlatformStatus": {"ParkBrakeSwActive": (index // 15) % 2},
        "ESPStatus": {"TractionControlOn": index % 2},
        "EBCMFrictionBrakeStatus": {"FrictionBrakeUnavailable": (index // 19) % 2},
        "ECMCruiseControl": {"CruiseSetSpeed": (20, 40, 60, 100)[(index // 13) % 4], "CruiseActive": (index // 9) % 2},
        "SPEED_RELATED": {"ClusterSpeed": speed * 3.6 + index % 3},
      }
      stock = {s.name: rng.randrange(1 << min(s.size, 16)) * s.factor + s.offset for s in packer.dbc.name_to_msg["PSCMStatus"].sigs.values()}
      stock.update(LKADriverAppldTrq=(0., 1., -1., 1.1, -1.1, 10.)[(index // 13) % 6], LKATorqueDeliveredStatus=(index // 11) % 4, RollingCounter=index % 16)
      messages["PSCMStatus"] = stock
      frames = []
      for name, values in messages.items():
        address, data, bus = packer.make_can_msg(name, 0, values)
        frames.append({"address": address, "data": list(data), "bus": bus})
      address, data, bus = packer.make_can_msg("ASCMActiveCruiseControlStatus", 2, {"ACCSpeedSetpoint": 80., "ACCCruiseState": (index // 7) % 8})
      frames.append({"address": address, "data": list(data), "bus": bus})
      if index % 4 == 0:
        address, data, _ = packer.make_can_msg("ASCMLKASteeringCmd", 0, {"RollingCounter": (index // 4) % 4, "LKASteeringCmd": 5})
        frames.extend({"address": address, "data": list(data), "bus": bus} for bus in (0, 2, 128))
      packets = [] if index % 71 == 70 else [{"mono_time": now, "frames": frames}]
      if 260 <= index < 315:
        packets = [{"mono_time": now, "frames": []}]
      control = car.CarControl.new_message(enabled=index % 100 < 80, latActive=index % 90 < 75, longActive=index % 80 < 65,
        orientationNED=[0., (0., 0.01, 0.1, -0.1)[(index // 12) % 4], 0.],
        cruiseControl={"cancel": index % 50 < 20, "resume": index % 40 < 6},
        hudControl={"setSpeed": (0., 20., 70., 71.)[(index // 11) % 4], "leadVisible": index % 3 == 0, "leadDistanceBars": 1 + (index // 20) % 4, "visualAlert": "fcw" if index % 9 == 0 else "none"},
        actuators={"torque": (0., 0.5, -0.5, 1., -1., 1.5)[(index // 13) % 6], "accel": (-4.5, -4., -1.2, -0.3, 0., 0.3, 1., 2., 3.)[(index // 13) % 9],
          "longControlState": ("off", "pid", "stopping", "starting")[(index // 17) % 4]})
      settings = {"LongPitch": str((index // 65) % 2), "EVTable": str((index // 77) % 2), "IsMetric": str((index // 53) % 2)}
      if index in (10, 90, 150, 230):
        settings.update(CustomSteerMax={10: "400", 90: "0", 150: "-1", 230: "280"}[index], CustomSteerDeltaUp=str(3 + index % 17), CustomSteerDeltaDown=str(5 + index % 13))
      steps.append({"now": now, "packets": packets, "control": list(control.to_bytes()), "settings": settings,
        "is_metric": profile % 2 == 0, "soft_hold": index % 3, "commit": {"vCruise": 70., "activateCruise": int(index % 25 < 5), "softHoldActive": index % 3}})
    pt = []
    pt.extend([[608, 8], [0x142, 8]])
    if profile % 2:
      pt.append([0xbe, 8])
    result.append({"name": f"runtime-{candidate}-{alpha}", "op": "runtime", "candidate": str(candidate), "alpha_long": alpha,
      "fingerprints": [[0, pt], [2, [[0x320, 8]] if profile % 4 else []]], "firmware": [],
      "settings": {"AutoEngage": "2", "UseLaneLineSpeed": "15", "LongActuatorDelay": "50", "LateralTorqueCustom": str(profile % 2)}, "now": 2_000_000_000, "steps": steps})
  runtime = [case for case in result if case["op"] == "runtime"]
  for case in runtime:
    previous = dict(case["settings"])
    for step in case["steps"]:
      changed = {key: value for key, value in step["settings"].items() if previous.get(key) != value}
      previous.update(step["settings"])
      step["settings"] = changed
  base = next(case for case in runtime if case["candidate"] == "CHEVROLET_VOLT" and not case["alpha_long"])
  for field in ("accel", "torque"):
    steps = [{**step} for step in base["steps"][:(1 if field == "accel" else 7)]]
    step = steps[-1]
    with car.CarControl.from_bytes(bytes(step["control"])) as control:
      control = control.as_builder()
      setattr(control.actuators, field, float("nan"))
      step["control"] = list(control.to_bytes())
    result.append({**base, "name": f"{field}-nan", "op": "numeric_error", "steps": steps})
  for case in runtime:
    result.append({**case, "name": case["name"].replace("runtime", "pedal-missing-message"), "op": "state_error", "fingerprints": [[0, [[0x201, 6], *case["fingerprints"][0][1]]], case["fingerprints"][1]], "steps": case["steps"][:1]})
    if case["candidate"] != "CHEVROLET_SUBURBAN_CC":
      result.append({**case, "name": case["name"].replace("runtime", "before-update"), "op": "before_update", "steps": case["steps"][:1]})
  return result
