from __future__ import annotations

import itertools
import random
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from opendbc.car.ford.values import CAR
  from openpilot.cereal import car

  result = []
  for candidate, alpha, offset, automatic, bsm in itertools.product(CAR, (False, True), (0, 4), (False, True), (False, True)):
    fingerprints = [[offset, [[0x5a, 8]] if automatic else [[0x123, 8]]]]
    if bsm:
      fingerprints[0][1].extend([[0x3a6, 8], [0x3a7, 8]])
    result.append({"name": f"params-{candidate}-{alpha}-{offset}-{automatic}-{bsm}", "op": "params",
      "candidate": str(candidate), "alpha_long": alpha, "fingerprints": fingerprints, "firmware": [],
      "settings": {"NNFF": str(int(bsm)), "NNFFLite": str(int(automatic)), "DisableMinSteerSpeed": str(int(alpha))},
      "now": 2_000_000_000, "steps": []})
  for request, version in itertools.product(([0x22, 0xde, 1], [0x22, 0xde, 2]), ([], [255] * 23, [255] * 24, [0] * 24, [255] * 7 + [0] + [255] * 16)):
    case = dict(result[0])
    case.update(name=f"eps-{request}-{version}", firmware=[{"ecu": "eps", "fw_version": version, "request": [request]}])
    result.append(case)
  for camera in ([], [[0x3d6, 8], [0x186, 8]], [[0x3d6, 16], [0x186, 8]], [[0x3d6, 8]], [[0x186, 16]]):
    case = dict(result[0])
    case.update(name=f"secoc-{camera}", candidate="FORD_F_150_MK14", fingerprints=[[2, camera]])
    result.append(case)
  missing = {"FORD_ESCAPE_MK4_5", "FORD_EXPEDITION_MK4"}
  rng = random.Random(177114)
  for profile, candidate in enumerate(c for c in CAR if str(c) not in missing):
    offset = 4 if profile % 2 else 0
    automatic = profile % 3 != 0
    packer = CANPacker("ford_lincoln_base_pt")
    steps = []
    for index in range(520):
      now = 2_000_000_000 + index * 10_000_000
      frames = []
      messages = {
        "SteeringPinion_Data": {"StePinCompAnEst_D_Qf": index % 4, "StePinComp_An_Est": (index % 41 - 20) * 4.},
        "BrakeSysFeatures": {"Veh_V_ActlBrk": (0., 0.36, 3.6, 10.8, 18., 32.4, 32.5, 90., 126.)[(index // 11) % 9]},
        "Yaw_Data_FD1": {"VehYaw_W_Actl": (index % 17 - 8) * 0.02},
        "DesiredTorqBrk": {"VehStop_D_Stat": index % 4, "PrkBrkStatus": (index // 3) % 4},
        "EngVehicleSpThrottle": {"ApedPos_Pc_ActlArb": (0., 0.1, 50., 100.)[(index // 7) % 4], "EngAout_N_Actl": index * 7},
        "BrakeSnData_4": {"BrkTot_Tq_Actl": index * 31},
        "EngBrakeData": {"BpedDrvAppl_D_Actl": index % 4, "Veh_V_DsplyCcSet": 85., "CcStat_D_Actl": (index // 8) % 8, "AccStopMde_D_Rq": (index // 5) % 4},
        "EPAS_INFO": {"SteeringColumnTorque": (0., 1., 1.1, -1., -1.1, 5.)[(index // 9) % 6], "EPAS_Failure": (index // 7) % 4},
        "Cluster_Info1_FD1": {"DrvSlipCtlMde_D_Rq": index % 4, "AccEnbl_B_RqDrv": index % 2},
        "Lane_Assist_Data3_FD1": {"LatCtlSte_D_Stat": (index // 3) % 8},
        "INSTRUMENT_PANEL": {"METRIC_UNITS": (index // 17) % 2},
        "PowertrainData_10": {"TrnRng_D_Rq": (index // 13) % 16},
        "Engine_Clutch_Data": {"CluPdlPos_Pc_Meas": index % 100},
        "BCM_Lamp_Stat_FD1": {"RvrseLghtOn_B_Stat": (index // 19) % 2},
        "BodyInfo_3_FD1": {"DrStatDrv_B_Actl": index % 2, "DrStatPsngr_B_Actl": (index // 2) % 2, "DrStatRl_B_Actl": (index // 3) % 2, "DrStatRr_B_Actl": (index // 4) % 2},
        "RCMStatusMessage2_FD1": {"FirstRowBuckleDriver": (index // 11) % 4},
        "Side_Detect_L_Stat": {"SodDetctLeft_D_Stat": (index // 4) % 4},
        "Side_Detect_R_Stat": {"SodDetctRight_D_Stat": (index // 5) % 4},
      }
      for name in ("Steering_Data_FD1", "ACCDATA_3", "IPMA_Data", "ACCDATA", "ACCDATA_2"):
        messages[name] = {s.name: rng.randrange(1 << min(s.size, 16)) * s.factor + s.offset for s in packer.dbc.name_to_msg[name].sigs.values()}
      messages["Steering_Data_FD1"].update(TurnLghtSwtch_D_Stat=index % 4, TjaButtnOnOffPress=(index // 14) % 2, AccButtnGapTogglePress=(index // 9) % 2)
      for name, values in messages.items():
        camera = name in ("ACCDATA_3", "IPMA_Data", "ACCDATA", "ACCDATA_2")
        address, data, bus = packer.make_can_msg(name, offset + (2 if camera else 0), values)
        frames.append({"address": address, "data": list(data), "bus": bus})
        if name.startswith("Side_Detect"):
          frames.append({"address": address, "data": list(data), "bus": offset + 2})
      packets = [] if index % 71 == 70 else [{"mono_time": now, "frames": frames}]
      if 360 <= index < 410:
        packets = [{"mono_time": now, "frames": []}]
      control = car.CarControl.new_message(enabled=index % 100 < 80, latActive=index % 130 < 110, longActive=index % 150 < 120,
        cruiseControl={"cancel": index % 60 < 9, "resume": index % 40 < 11},
        orientationNED=[0., (-0.1, 0., 0.1)[(index // 27) % 3], 0.] if index % 2 else [],
        hudControl={"visualAlert": ("none", "steerRequired", "ldw", "fcw")[(index // 17) % 4], "leftLaneVisible": index % 2 == 0,
          "rightLaneVisible": index % 3 == 0, "leftLaneDepart": index % 7 == 0, "rightLaneDepart": index % 11 == 0,
          "leadVisible": index % 5 == 0, "leadDistanceBars": 2 if index < 450 else 3},
        actuators={"curvature": (0., 0.04, -0.04, 0.001, -0.001)[(index // 21) % 5],
          "accel": (-4., -3.5, -0.51, -0.5, -0.49, 0., 0.1, 0.2, 0.3, 2., 4.)[(index // 13) % 11],
          "longControlState": ("off", "pid", "stopping", "starting")[(index // 19) % 4]})
      steps.append({"now": now, "packets": packets, "control": list(control.to_bytes()), "settings": {},
        "is_metric": profile % 2 == 0, "soft_hold": index % 4, "commit": {"vCruise": 70., "activateCruise": index % 2}})
    fingerprints = [[offset, [[0x5a if automatic else 0x123, 8], [0x3a6, 8], [0x3a7, 8]]]]
    result.append({"name": f"runtime-{candidate}", "op": "runtime", "candidate": str(candidate), "alpha_long": profile % 2 == 0,
      "fingerprints": fingerprints, "firmware": [], "settings": {}, "now": 2_000_000_000, "steps": steps})
  runtime = next(case for case in result if case["op"] == "runtime")
  for pitch in (-0.1, 0., 0.1):
    control = car.CarControl.new_message(longActive=True, orientationNED=[0., pitch, 0.], actuators={"accel": float("nan")})
    step = {**runtime["steps"][0], "control": list(control.to_bytes())}
    result.append({**runtime, "name": f"accel-nan-pitch-{pitch}", "op": "numeric_error", "steps": [step]})
  for pitch in (float("inf"), float("-inf")):
    control = car.CarControl.new_message(longActive=True, orientationNED=[0., pitch, 0.])
    step = {**runtime["steps"][0], "control": list(control.to_bytes())}
    result.append({**runtime, "name": f"pitch-{pitch}", "op": "numeric_error", "steps": [step]})
  for cancel in (False, True):
    control = car.CarControl.new_message(cruiseControl={"cancel": cancel})
    step = {**runtime["steps"][0], "control": list(control.to_bytes()), "packets": []}
    result.append({**runtime, "name": f"before-update-{cancel}", "op": "before_update", "steps": [step]})
  return result
