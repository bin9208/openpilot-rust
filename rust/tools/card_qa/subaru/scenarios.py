from __future__ import annotations

import itertools
import random
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from opendbc.car import Bus
  from opendbc.car.subaru.values import CAR, DBC, SubaruFlags as F
  from openpilot.cereal import car

  result = []
  for candidate, alpha, detection, settings in itertools.product(CAR, (False, True), (False, True), ({}, {"NNFF": "1", "NNFFLite": "1", "DisableMinSteerSpeed": "1"})):
    result.append({"name": f"params-{candidate}-{alpha}-{detection}-{bool(settings)}", "op": "params", "candidate": str(candidate),
                   "alpha_long": alpha, "fingerprints": [[0, [[0x228, 8], [0x25c, 8]]], [2, [[0x323, 8]]]] if detection else [],
                   "firmware": [], "settings": settings, "now": 2_000_000_000, "steps": []})
  rng = random.Random(177124)
  for candidate, alpha in itertools.product(CAR, (False, True)):
    flags = candidate.config.flags
    preglobal, hybrid, gen2 = bool(flags & F.PREGLOBAL), bool(flags & F.HYBRID), bool(flags & F.GLOBAL_GEN2)
    packer = CANPacker(DBC[candidate][Bus.pt])
    messages = [(0, "Dashlights"), (0, "BodyInfo"), (0, "Steering_Torque"), (0, "BSD_RCTA"),
                (1 if hybrid else 0, "Transmission"), (1 if hybrid else 0, "Throttle_Hybrid" if hybrid else "Throttle"),
                (1 if gen2 else 0, "Wheel_Speeds"), (2, "ES_DashStatus")]
    if preglobal:
      messages += [(0, "Brake_Pedal"), (0, "Dash_State2"), (2, "ES_Distance"), (0, "CruiseControl")]
    else:
      messages += [(1 if gen2 else 0, "Brake_Status"), (2, "ES_LKAS_State"), (1 if gen2 else 2, "ES_Brake"), (2, "ES_Infotainment")]
      if not hybrid:
        messages += [(1 if gen2 else 2, "ES_Distance"), (1 if gen2 else 2, "ES_Status"), (1 if gen2 else 0, "CruiseControl")]
    steps = []
    for index in range(360):
      now = 2_000_000_000 + index * 10_000_000
      frames = []
      for bus, name in messages:
        signals = packer.dbc.name_to_msg[name].sigs
        values = {key: rng.randrange(1 << min(signal.size, 12)) * signal.factor + signal.offset for key, signal in signals.items()}
        if index < 120 or index >= 210:
          for key, signal in signals.items():
            if key == 'COUNTER': values[key] = index % (1 << signal.size)
        if name == "Steering_Torque":
          values.update(Steering_Angle=((index % 120) * 0.6), Steer_Torque_Sensor=(0, 60, 75, 80, 81, -81, 300, -300)[index // 11 % 8])
          if "COUNTER" in signals: values["COUNTER"] = index % 16
        elif name == "Wheel_Speeds":
          values.update({key: (0., 1., 30., 60., 100.)[index // 17 % 5] for key in ("FL", "FR", "RL", "RR")})
        elif name == "Dashlights":
          values.update(LEFT_BLINKER=int(index % 120 == 0), RIGHT_BLINKER=int(index % 120 == 60))
        elif name == "ES_LKAS_State":
          values.update(LKAS_Alert=(0, 1, 2, 11, 12, 27, 28, 30)[index // 10 % 8], LKAS_Alert_Msg=(0, 1, 7)[index // 10 % 3])
        elif name == "ES_DashStatus" and not preglobal:
          values.update(LKAS_State_Msg=index // 10 % 4, Cruise_State=index // 20 % 4)
        elif name == "ES_Infotainment":
          values["LKAS_State_Infotainment"] = index // 10 % 5
        address, data, source = packer.make_can_msg(name, bus, values)
        frames.append({"address": address, "data": list(data), "bus": source})
      packets = [{"mono_time": now, "frames": frames}]
      if 170 <= index < 210: packets = [{"mono_time": now, "frames": []}]
      if index % 71 == 70: packets = []
      control = car.CarControl.new_message(
        enabled=index % 100 < 80, latActive=index % 90 < 75, longActive=index % 70 < 55,
        cruiseControl={"cancel": index % 50 < 15, "resume": index % 40 < 12},
        actuators={"torque": (0., 0.5, -0.5, 1., -1., 2., -2.)[index // 21 % 7], "accel": (-4., -3.5, -0.41, 0., 1.99, 2., 4.)[index // 13 % 7]},
        hudControl={"visualAlert": ("none", "steerRequired", "ldw", "fcw")[index // 10 % 4], "leftLaneVisible": index % 3 == 0,
                    "rightLaneVisible": index % 4 == 0, "leftLaneDepart": index % 3 == 0, "rightLaneDepart": index % 3 != 0, "leadVisible": index % 2 == 0})
      steps.append({"now": now, "packets": packets, "control": list(control.to_bytes()), "settings": {}, "is_metric": index % 2 == 0,
                    "soft_hold": index % 4, "commit": {"vCruise": 70., "activateCruise": index % 2}})
    result.append({"name": f"runtime-{candidate}-{alpha}", "op": "runtime", "candidate": str(candidate), "alpha_long": alpha,
                   "fingerprints": [[0, [[0x228, 8], [0x25c, 8]]], [2, [[0x323, 8]]]], "firmware": [], "settings": {}, "now": 2_000_000_000, "steps": steps})
  profiles = [case for case in result if case['op'] == 'runtime']
  for profile, cancel in itertools.product(profiles[::2], (False, True)):
    control = car.CarControl.new_message(enabled=True, latActive=True, cruiseControl={'cancel': cancel}, actuators={'torque': 1.})
    step = {**profile['steps'][0], 'packets': [], 'control': list(control.to_bytes())}
    result.append({**profile, 'op': 'before_update', 'name': f"before-update-{profile['candidate']}-{cancel}", 'steps': [step]})
  for profile in profiles:
    if profile['candidate'] == 'SUBARU_OUTBACK' and not profile['alpha_long']:
      result.append({**profile, 'name': 'runtime-gen2-forced-long-disabled-eyesight', 'flags_or': int(F.DISABLE_EYESIGHT), 'force_long': True})
      break
  return result
