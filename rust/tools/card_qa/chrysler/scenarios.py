from __future__ import annotations

import itertools
import math
import random
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from openpilot.cereal import car

  candidates = (
    "CHRYSLER_PACIFICA_2018_HYBRID",
    "CHRYSLER_PACIFICA_2019_HYBRID",
    "CHRYSLER_PACIFICA_2018",
    "CHRYSLER_PACIFICA_2020",
    "DODGE_DURANGO",
    "JEEP_GRAND_CHEROKEE",
    "JEEP_GRAND_CHEROKEE_2019",
    "RAM_1500_5TH_GEN",
    "RAM_HD_5TH_GEN",
  )
  result = []
  for candidate, alpha, nnff, lite, disable, bsm, fw in itertools.product(
    candidates, (False, True), (False, True), (False, True), (False, True), (False, True), (False, True)
  ):
    result.append(
      {
        "name": f"params-{candidate}-{alpha}-{nnff}-{lite}-{disable}-{bsm}-{fw}",
        "op": "params",
        "candidate": candidate,
        "alpha_long": alpha,
        "fingerprints": [(0, [(720, 8)])] if bsm else [],
        "firmware": [{"ecu": "eps", "fw_version": list(b"6841-owned")}] if fw else [],
        "settings": {"NNFF": str(int(nnff)), "NNFFLite": str(int(lite)), "DisableMinSteerSpeed": str(int(disable))},
        "now": 2_000_000_000,
        "steps": [],
      }
    )
  for candidate, prefix in itertools.product(candidates, (b"", b"68", b"6800", b"6831", b"6832", b"6840", b"6841", b"\xff\x00\n")):
    result.append(
      {
        "name": f"firmware-{candidate}-{prefix.hex()}",
        "op": "params",
        "candidate": candidate,
        "alpha_long": False,
        "fingerprints": [(1, [(720, 8)])],
        "firmware": [{"ecu": "eps", "fw_version": list(prefix)}],
        "settings": {},
        "now": 2_000_000_000,
        "steps": [],
      }
    )
  for candidate in candidates:
    result.append(
      {
        "name": f"non-eps-{candidate}",
        "op": "params",
        "candidate": candidate,
        "alpha_long": False,
        "fingerprints": [],
        "firmware": [{"ecu": "engine", "fw_version": list(b"6841")}],
        "settings": {},
        "now": 2_000_000_000,
        "steps": [],
      }
    )
  profiles = [(candidate, [], {"NNFF": str(index % 2), "NNFFLite": str((index // 2) % 2)}, bool(index % 2)) for index, candidate in enumerate(candidates)]
  profiles.extend(
    [
      ("CHRYSLER_PACIFICA_2018", [{"ecu": "eps", "fw_version": list(b"6841")}], {}, True),
      ("RAM_1500_5TH_GEN", [{"ecu": "eps", "fw_version": list(b"6831")}], {}, False),
      ("RAM_HD_5TH_GEN", [], {"DisableMinSteerSpeed": "1", "NNFF": "1"}, True),
    ]
  )
  rng = random.Random(177990)
  for profile, (candidate, firmware, settings, bsm) in enumerate(profiles):
    ram = candidate in ("RAM_1500_5TH_GEN", "RAM_HD_5TH_GEN")
    dbc = (
      "chrysler_ram_dt_generated"
      if candidate == "RAM_1500_5TH_GEN"
      else ("chrysler_ram_hd_generated" if candidate == "RAM_HD_5TH_GEN" else "chrysler_pacifica_2017_hybrid_generated")
    )
    packer = CANPacker(dbc)

    def packed(name, bus, values, packer=packer):
      address, data, source = packer.make_can_msg(name, bus, values)
      return {"address": address, "data": list(data), "bus": source}

    steps = []
    for index in range(800):
      now = 2_000_000_000 + index * 10_000_000
      if index < 80:
        speed = 0.0
      elif index < 290:
        speed = 25.0
      elif index < 340:
        speed = 10.0
      elif index < 590:
        speed = 25.0
      elif index < 620:
        speed = 2.0
      else:
        speed = 25.0
      frames = [
        packed("CRUISE_BUTTONS", 0, {"ACC_Distance_Dec": int(index % 13 < 6), "COUNTER": index % 16}),
        packed(
          "BCM_1",
          0,
          {
            "DOOR_OPEN_FL": int(index % 31 == 0),
            "DOOR_OPEN_FR": int(index % 32 == 0),
            "DOOR_OPEN_RL": int(index % 33 == 0),
            "DOOR_OPEN_RR": int(index % 34 == 0),
          },
        ),
        packed("ORC_1", 0, {"SEATBELT_DRIVER_UNLATCHED": index % 2}),
        packed("ESP_1", 0, {"Brake_Pedal_State": index % 4}),
        packed("ECM_5", 0, {"Accelerator_Position": (0.0, 0.001, 0.5, 1.0)[(index // 15) % 4]}),
        packed("ESP_6", 0, {"WHEEL_SPEED_FL": speed + 0.1, "WHEEL_SPEED_FR": speed + 0.2, "WHEEL_SPEED_RL": speed + 0.3, "WHEEL_SPEED_RR": speed + 0.4}),
        packed("STEERING_LEVERS", 0, {"TURN_SIGNALS": (0, 1, 0, 2, 3)[(index // 17) % 5], "HIGH_BEAM_PRESSED": index % 2}),
        packed(
          "STEERING", 0, {"STEERING_ANGLE": math.sin(index / 11.0) * 150, "STEERING_ANGLE_HP": index % 3 - 1, "STEERING_RATE": math.cos(index / 12.0) * 80}
        ),
        packed(
          "EPS_2",
          0,
          {
            "COLUMN_TORQUE": (0, 120, 121, -120, -121, 500, -500)[(index // 11) % 7],
            "EPS_TORQUE_MOTOR": (0, 79, 80, 81, -79, -80, -81, 500, -500)[(index // 21) % 9],
            "LKAS_TEMPORARY_FAULT": index % 2,
            "LKAS_STATE": (0, 1, 4, 5)[(index // 35) % 4],
          },
        ),
        packed(
          "DAS_3", 2 if ram else 0, {"ACC_AVAILABLE": index % 2, "ACC_ACTIVE": int(index % 140 < 105), "ACC_STANDSTILL": index % 2, "ACC_FAULTED": index % 4}
        ),
        packed("DAS_4", 2 if ram else 0, {"ACC_SET_SPEED_KPH": (0, 1, 29, 65, 157, 255)[(index // 15) % 6], "ACC_STATE": index % 8}),
        packed("DAS_6", 2, {"CAR_MODEL": index % 256, "AUTO_HIGH_BEAM_ON": index % 2}),
        packed("BSM_1", 0, {"LEFT_STATUS": index % 8, "RIGHT_STATUS": (index + 1) % 8}),
      ]
      if ram:
        frames.extend(
          [
            packed("ESP_8", 0, {"Vehicle_Speed": speed * 3.6}),
            packed("Transmission_Status", 0, {"Gear_State": (0, 1, 2, 3, 4, 5, 6, 7)[(index // 19) % 8]}),
            packed("EPS_3", 0, {"DASM_FAULT": index % 2}),
          ]
        )
      else:
        frames.extend(
          [
            packed("SPEED_1", 0, {"SPEED_LEFT": speed, "SPEED_RIGHT": speed + (0.1 if speed else 0.0)}),
            packed("GEAR", 0, {"PRNDL": (0, 1, 2, 3, 4, 5, 6, 7)[(index // 19) % 8]}),
          ]
        )
      if index in (255, 256):
        next(f for f in frames if f["address"] == packer.dbc.name_to_msg["STEERING"].address)["data"][-1] ^= 1
      if 100 <= index < 116:
        replacement = packed("CRUISE_BUTTONS", 0, {"COUNTER": 9, "ACC_Distance_Dec": 0})
        frames = [replacement if f["address"] == replacement["address"] else f for f in frames]
      if 180 <= index < 240:
        frames = [f for f in frames if f["address"] != packer.dbc.name_to_msg["DAS_6"].address]
      packets = [] if index % 71 == 70 else [{"mono_time": now, "frames": frames}]
      if 420 <= index < 480:
        packets = [{"mono_time": now, "frames": []}]
      control = car.CarControl.new_message(
        enabled=index % 130 < 110,
        latActive=not (670 <= index < 690),
        longActive=True,
        cruiseControl={"cancel": index % 100 < 17, "resume": index % 100 < 40},
        hudControl={"visualAlert": ("none", "steerRequired", "ldw")[(index // 50) % 3]},
        actuators={"torque": (0.0, 0.5, -0.5, 1.0, -1.0, 1.4, -1.4)[(index // 37) % 7], "accel": rng.uniform(-3.0, 2.0)},
      )
      if index in (510, 511):
        control.actuators.torque = 1e20 if index == 510 else -1e20
      steps.append(
        {
          "now": now,
          "packets": packets,
          "control": list(control.to_bytes()),
          "settings": {},
          "is_metric": profile % 2 == 0,
          "soft_hold": index % 4,
          "commit": {"vCruise": 70.0, "activateCruise": index % 2},
        }
      )
    result.append(
      {
        "name": f"runtime-{profile}-{candidate}",
        "op": "runtime",
        "candidate": candidate,
        "alpha_long": True,
        "fingerprints": [(0, [(720, 8)])] if bsm else [],
        "firmware": firmware,
        "settings": settings,
        "now": 2_000_000_000,
        "steps": steps,
      }
    )
  return result
