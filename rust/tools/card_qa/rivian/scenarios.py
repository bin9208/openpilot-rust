from __future__ import annotations

import itertools
import math
import random
from can_source import load


def cases():
  load()
  from opendbc.can import CANPacker
  from openpilot.cereal import car

  result = []
  for alpha, nnff, lite, disable, fw in itertools.product((False, True), repeat=5):
    result.append(
      {
        "name": f"params-{alpha}-{nnff}-{lite}-{disable}-{fw}",
        "op": "params",
        "candidate": "RIVIAN_R1_GEN1",
        "alpha_long": alpha,
        "fingerprints": [],
        "firmware": [{"ecu": "eps", "fw_version": list(b"owned-rivian-firmware")}] if fw else [],
        "settings": {"NNFF": str(int(nnff)), "NNFFLite": str(int(lite)), "DisableMinSteerSpeed": str(int(disable))},
        "now": 2_000_000_000,
        "steps": [],
      }
    )
  rng = random.Random(177123)
  for profile, (alpha, metric) in enumerate(itertools.product((False, True), (False, True))):
    packer = CANPacker("rivian_primary_actuator")

    def packed(name, bus, values, packer=packer):
      address, data, source = packer.make_can_msg(name, bus, values)
      return {"address": address, "data": list(data), "bus": source}

    def stock(name, packer=packer):
      return {s.name: rng.randrange(1 << min(s.size, 16)) * s.factor + s.offset for s in packer.dbc.name_to_msg[name].sigs.values()}

    steps = []
    for index in range(640):
      now = 2_000_000_000 + index * 10_000_000
      speed = (0.0, 0.005, 0.01, 8.99, 9.0, 9.01, 12.5, 16.99, 17.0, 17.01, 30.0)[(index // 12) % 11]
      lka = stock("ACM_lkaHbaCmd")
      lka.update(ACM_hbaSysState=index % 8, ACM_hbaLamp=index % 2, ACM_hbaOnOffState=index % 4, ACM_slifOnOffState=(index // 2) % 4)
      vdm = stock("VDM_AdasSts")
      vdm.update(
        VDM_AdasStatus_Counter=index % 16,
        VDM_AdasInterfaceStatus=index % 4,
        VDM_AdasFaultStatus=(0, 1, 3, 4, 15)[(index // 17) % 5],
        VDM_AdasAccelRequestAcknowledged=index % 4,
      )
      frames = [
        packed("ESP_Status", 0, {"ESP_Vehicle_Speed": speed * 3.6}),
        packed("VDM_PropStatus", 0, {"VDM_AcceleratorPedalPosition": (0.0, 0.1, 100.0)[(index // 15) % 3], "VDM_Prndl_Status": index % 16}),
        packed("ESPiB3", 0, {"ESPiB3_pMC1": (-30.0, 0.0, 0.3, 125.0, 276.0)[(index // 11) % 5]}),
        packed("iBESP2", 0, {"iBESP2_BrakePedalApplied": index % 4}),
        packed(
          "EPAS_AdasStatus",
          0,
          {"EPAS_InternalSas": math.sin(index / 11.0) * 100, "EPAS_SteeringAngleSpeed": math.cos(index / 12.0) * 70, "EPAS_EacErrorCode": index % 16},
        ),
        packed("EPAS_SystemStatus", 0, {"EPAS_TorsionBarTorque": (0.0, 1.0, 1.01, -1.0, -1.01, 20.0, -20.0)[(index // 11) % 7]}),
        packed("VDM_AdasSts", 0, vdm),
        packed("RCM_Status", 0, {"RCM_Status_IND_WARN_BELT_DRIVER": index % 4}),
        packed(
          "SCCM_WheelTouch",
          0,
          {
            "SCCM_WheelTouch_Counter": index % 16,
            "SCCM_WheelTouch_HandsOn": index % 2,
            "SCCM_WheelTouch_CapacitiveValue": index * 7 % 4096,
            "SETME_X52": index % 256,
          },
        ),
        packed("ACM_tsrCmd", 1, {"ACM_tsrSpdDisClsMain": (0, 1, 30, 84, 85, 86, 255)[(index // 15) % 7]}),
        packed(
          "IndicatorLights",
          1,
          {
            "RearDriverDoor": index % 4,
            "FrontPassengerDoor": (index // 2) % 4,
            "DriverDoor": (index // 3) % 4,
            "RearPassengerDoor": (index // 4) % 4,
            "TurnLightLeft": index % 4,
            "TurnLightRight": (index + 1) % 4,
          },
        ),
        packed("ACM_Status", 2, {"ACM_FeatureStatus": index % 4, "ACM_FaultStatus": (index // 2) % 4}),
        packed("ACM_AebRequest", 2, {"ACM_EnableRequest": index % 4}),
        packed("ACM_lkaHbaCmd", 2, lka),
      ]
      if 180 <= index < 240:
        frames = [f for f in frames if f["address"] != packer.dbc.name_to_msg["ACM_lkaHbaCmd"].address]
      packets = [] if index % 71 == 70 else [{"mono_time": now, "frames": frames}]
      if 360 <= index < 420:
        packets = [{"mono_time": now, "frames": []}]
      control = car.CarControl.new_message(
        enabled=index % 100 < 80,
        latActive=index % 130 < 110,
        longActive=index % 150 < 120,
        cruiseControl={"cancel": index % 50 < 17, "resume": index % 40 < 13},
        actuators={
          "torque": (0.0, 0.5, -0.5, 1.0, -1.0, 1.5, -1.5)[(index // 31) % 7],
          "accel": (-4.0, -3.5, -3.49, 0.0, 1.99, 2.0, 2.01, 4.0)[(index // 19) % 8],
        },
      )
      if index in (510, 511):
        control.actuators.torque = 1e20 if index == 510 else -1e20
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
        "name": f"runtime-{alpha}-{metric}",
        "op": "runtime",
        "candidate": "RIVIAN_R1_GEN1",
        "alpha_long": alpha,
        "fingerprints": [],
        "firmware": [],
        "settings": {"NNFF": str(profile % 2), "NNFFLite": str((profile // 2) % 2)},
        "now": 2_000_000_000,
        "steps": steps,
      }
    )
  for alpha, active in itertools.product((False, True), repeat=2):
    control = car.CarControl.new_message(enabled=True, latActive=active, actuators={"torque": 1.0})
    result.append(
      {
        "name": f"before-update-{alpha}-{active}",
        "op": "before_update",
        "candidate": "RIVIAN_R1_GEN1",
        "alpha_long": alpha,
        "fingerprints": [],
        "firmware": [],
        "settings": {},
        "now": 2_000_000_000,
        "steps": [
          {
            "now": 2_000_000_000,
            "packets": [],
            "control": list(control.to_bytes()),
            "settings": {},
            "is_metric": True,
            "soft_hold": 0,
            "commit": {"vCruise": 0.0, "activateCruise": 0},
          }
        ],
      }
    )
  return result
