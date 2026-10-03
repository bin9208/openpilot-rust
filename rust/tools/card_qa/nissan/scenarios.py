from __future__ import annotations

import itertools
import math
import random
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from openpilot.cereal import car

  candidates = ("NISSAN_XTRAIL", "NISSAN_LEAF", "NISSAN_LEAF_IC", "NISSAN_ROGUE", "NISSAN_ALTIMA")
  result = []
  for candidate, alpha, nnff, lite, disable, fw in itertools.product(candidates, (False, True), (False, True), (False, True), (False, True), (False, True)):
    firmware = [{"ecu": "eps", "fw_version": list(b"owned-nissan-firmware")}] if fw else []
    result.append(
      {
        "name": f"params-{candidate}-{alpha}-{nnff}-{lite}-{disable}-{fw}",
        "op": "params",
        "candidate": candidate,
        "alpha_long": alpha,
        "fingerprints": [],
        "firmware": firmware,
        "settings": {"NNFF": str(int(nnff)), "NNFFLite": str(int(lite)), "DisableMinSteerSpeed": str(int(disable))},
        "now": 2_000_000_000,
        "steps": [],
      }
    )
  rng = random.Random(177185)
  for profile, (candidate, metric) in enumerate(itertools.product(candidates, (True, False))):
    leaf = candidate in ("NISSAN_LEAF", "NISSAN_LEAF_IC")
    altima = candidate == "NISSAN_ALTIMA"
    pt = int(altima)
    camera = int(not altima)
    packer = CANPacker("nissan_leaf_2018_generated" if leaf else "nissan_x_trail_2017_generated")

    def packed(name, bus, values, packer=packer):
      address, data, source = packer.make_can_msg(name, bus, values)
      return {"address": address, "data": list(data), "bus": source}

    def stock(name, packer=packer):
      return {signal.name: rng.randrange(1 << min(signal.size, 16)) * signal.factor + signal.offset for signal in packer.dbc.name_to_msg[name].sigs.values()}

    steps = []
    for index in range(560):
      now = 2_000_000_000 + index * 10_000_000
      speed = (0.0, 4.99, 5.0, 5.01, 14.99, 15.0, 15.01, 30.0, 60.0)[(index // 9) % 9]
      rear = speed * 3.6
      front = [rear + delta for delta in (0.015, 10.0)]
      if index % 81 == 0:
        front = [0.0, 0.0]
      throttle = stock("CRUISE_THROTTLE")
      throttle.update(
        COUNTER=index % 4,
        FOLLOW_DISTANCE_BUTTON=int(index % 13 < 6),
        GAS_PEDAL=(0, 3, 4, 100)[(index // 15) % 4],
        USER_BRAKE_PRESSED=int(index % 65 < 8),
        CANCEL_BUTTON=index % 2,
        SET_BUTTON=(index // 2) % 2,
        RES_BUTTON=(index // 3) % 2,
        PROPILOT_BUTTON=(index // 4) % 2,
        NO_BUTTON_PRESSED=(index // 5) % 2,
      )
      if leaf:
        throttle["CRUISE_AVAILABLE"] = index % 2
      frames = [
        packed("CRUISE_THROTTLE", pt, throttle),
        packed("WHEEL_SPEEDS_FRONT", pt, {"WHEEL_SPEED_FL": front[0], "WHEEL_SPEED_FR": front[1]}),
        packed("WHEEL_SPEEDS_REAR", pt, {"WHEEL_SPEED_RL": rear, "WHEEL_SPEED_RR": rear + (0.005 if rear else 0.0)}),
        packed(
          "STEER_ANGLE_SENSOR", pt, {"STEER_ANGLE": (0.0, 599.9, 600.0, 600.1, -599.9, -600.0, -600.1, 750.0, -750.0)[(index // 10) % 9], "COUNTER": index % 16}
        ),
        packed(
          "STEER_TORQUE_SENSOR",
          camera if altima else pt,
          {"STEER_TORQUE_DRIVER": (0.0, 1.0, 1.01, -1.0, -1.01, 20.0, -20.0)[(index // 11) % 7], "COUNTER": index % 16},
        ),
        packed("CRUISE_STATE", pt if altima else 2, {"CRUISE_ENABLED": int(index % 140 < 105)}),
        packed("LIGHTS", pt, {"LEFT_BLINKER": int(index % 80 < 5), "RIGHT_BLINKER": int(index % 110 < 5)}),
        packed(
          "DOORS_LIGHTS",
          pt,
          dict(
            DOOR_OPEN_RR=int(index % 31 == 0),
            DOOR_OPEN_RL=int(index % 32 == 0),
            DOOR_OPEN_FR=int(index % 33 == 0),
            DOOR_OPEN_FL=int(index % 34 == 0),
            **({} if leaf else {"USER_BRAKE_PRESSED": int(index % 67 < 8)}),
          ),
        ),
        packed("ESP", pt, {"ESP_DISABLED": index % 2}),
        packed("GEARBOX", pt, {"GEAR_SHIFTER": (0, 1, 2, 3, 4, 5, 6, 7)[(index // 19) % 8]}),
        packed("LKAS_SETTINGS", pt if altima else 2, {"LKAS_ENABLED": index % 2}),
      ]
      if leaf:
        frames.append(packed("HUD_SETTINGS", pt, {"SPEED_MPH": int(not metric)}))
        frames.append(
          packed(
            "CANCEL_MSG",
            pt,
            {"CANCEL_SEATBELT": index % 2, "NEW_SIGNAL_1": index % 64, "NEW_SIGNAL_2": (index // 2) % 2, "NEW_SIGNAL_3": (index * 195731) % (1 << 48)},
          )
        )
        if candidate == "NISSAN_LEAF":
          frames.append(packed("SEATBELT", pt, {"SEATBELT_DRIVER_LATCHED": index % 2}))
      else:
        frames.append(packed("GAS_PEDAL", pt, {"GAS_PEDAL": (0, 3, 4, 700)[(index // 15) % 4]}))
        frames.append(packed("HUD", pt, {"SEATBELT_DRIVER_LATCHED": index % 2, "SPEED_MPH": int(not metric)}))
        frames.append(packed("PRO_PILOT", 2 if altima else camera, {"CRUISE_ON": index % 2, "COUNTER": index % 16}))
      hud = stock("PROPILOT_HUD")
      hud["SET_SPEED"] = (0, 1, 2, 29, 65, 157, 254, 255)[(index // 15) % 8]
      frames.append(packed("PROPILOT_HUD", pt if altima else 2, hud))
      if not altima:
        frames.append(packed("PROPILOT_HUD_INFO_MSG", 2, stock("PROPILOT_HUD_INFO_MSG")))
      if 180 <= index < 240:
        frames = [f for f in frames if f["address"] != packer.dbc.name_to_msg["PROPILOT_HUD"].address]
      packets = [] if index % 71 == 70 else [{"mono_time": now, "frames": frames}]
      if 330 <= index < 390:
        packets = [{"mono_time": now, "frames": []}]
      if index == 425:
        packets.append({"mono_time": now + 1, "frames": frames})
      control = car.CarControl.new_message(
        enabled=index % 130 < 110,
        latActive=index % 100 < 80,
        longActive=True,
        cruiseControl={"cancel": index % 100 < 17, "resume": index % 80 > 70},
        hudControl={
          "setSpeed": 20.0,
          "leftLaneVisible": index % 2 == 0,
          "rightLaneVisible": index % 3 == 0,
          "leftLaneDepart": index % 7 == 0,
          "rightLaneDepart": index % 5 == 0,
          "visualAlert": ("none", "steerRequired", "ldw")[(index // 50) % 3],
        },
        actuators={"torque": rng.uniform(-1.4, 1.4), "accel": rng.uniform(-3.0, 2.0), "steeringAngleDeg": math.sin(index / 21.0) * 1000},
      )
      if index in (40, 41):
        control.actuators.steeringAngleDeg = 1e20 if index == 40 else -1e20
      steps.append(
        {
          "now": now,
          "packets": packets,
          "control": list(control.to_bytes()),
          "settings": {},
          "is_metric": metric,
          "soft_hold": index % 4,
          "commit": {"vCruise": 70.0, "activateCruise": index % 2},
        }
      )
    result.append(
      {
        "name": f"runtime-{profile}-{candidate}-{metric}",
        "op": "runtime",
        "candidate": candidate,
        "alpha_long": True,
        "fingerprints": [(pt, [(389, 8)])],
        "firmware": [{"ecu": "eps", "fw_version": list(b"owned-nissan-firmware")}] if profile % 2 else [],
        "settings": {"NNFF": str(profile % 2), "NNFFLite": str((profile // 2) % 2)},
        "now": 2_000_000_000,
        "steps": steps,
      }
    )
  return result
