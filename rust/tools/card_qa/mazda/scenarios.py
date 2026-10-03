from __future__ import annotations

import itertools
import math
import random
from can_source import load


def cases():
    load()
    from opendbc.can import CANPacker
    from openpilot.cereal import car
    candidates = ("MAZDA_CX5", "MAZDA_CX9", "MAZDA_3", "MAZDA_6", "MAZDA_CX9_2021", "MAZDA_CX5_2022")
    result = []
    for candidate, alpha, nnff, lite, disable, fw in itertools.product(candidates, (False, True), (False, True), (False, True), (False, True), (False, True)):
        firmware = [dict(ecu="eps", fw_version=list(b"owned-mazda-firmware"))] if fw else []
        result.append(dict(name=f"params-{candidate}-{alpha}-{nnff}-{lite}-{disable}-{fw}", op="params", candidate=candidate, alpha_long=alpha, fingerprints=[], firmware=firmware, settings={"NNFF": str(int(nnff)), "NNFFLite": str(int(lite)), "DisableMinSteerSpeed": str(int(disable))}, now=2_000_000_000, steps=[]))
    packer = CANPacker("mazda_2017")
    def packed(name, bus, values):
        address, data, source = packer.make_can_msg(name, bus, values)
        return dict(address=address, data=list(data), bus=source)
    rng = random.Random(177)
    for profile, (candidate, metric) in enumerate(itertools.product(candidates, (True, False))):
        steps = []
        for index in range(520):
            now = 2_000_000_000 + index * 10_000_000
            speed = (0., .1, .11, 44.99, 45., 45.01, 52., 52.01, 60., 51., 44.)[(index // 9) % 11]
            wheels = [speed + offset for offset in (0., .01, .02, .03)]
            buttons = (0, 1, 2, 3, 7)[(index // 7) % 5]
            frames = [
                packed("CRZ_BTNS", 0, dict(CTR=index % 16, SET_P=int(buttons & 1 != 0), SET_M=int(buttons & 2 != 0), RES=int(buttons & 4 != 0), CAN_OFF=int(index % 19 == 0), DISTANCE_LESS=int(index % 13 < 6))),
                packed("WHEEL_SPEEDS", 0, dict(zip(("FL", "FR", "RL", "RR"), wheels))),
                packed("ENGINE_DATA", 0, dict(SPEED=speed, PEDAL_GAS=1 if index % 70 < 8 else 0)),
                packed("GEAR", 0, dict(GEAR=(0, 1, 2, 3, 4, 7)[(index // 30) % 6], GEAR_BOX=index % 8)),
                packed("BLINK_INFO", 0, dict(HIGH_BEAMS=index % 3, LEFT_BLINK=int(index % 80 < 5), RIGHT_BLINK=int(index % 110 < 5))),
                packed("BSM", 0, dict(LEFT_BS_STATUS=index % 4, RIGHT_BS_STATUS=(index + 1) % 4)),
                packed("STEER", 0, dict(STEER_ANGLE=math.sin(index / 11.) * 100)),
                packed("STEER_TORQUE", 0, dict(STEER_TORQUE_SENSOR=(0, 15, 16, -15, -16, 127, -127)[(index // 11) % 7], STEER_TORQUE_MOTOR=math.cos(index / 12.) * 700)),
                packed("STEER_RATE", 0, dict(STEER_ANGLE_RATE=math.sin(index / 13.) * 50, LKAS_BLOCK=int(index % 32 < 8))),
                packed("PEDALS", 0, dict(BRAKE_ON=int(index % 100 < 7), STANDSTILL=int(speed <= .1))),
                packed("BRAKE", 0, dict(BRAKE_PRESSURE=index % 100)),
                packed("SEATBELT", 0, dict(DRIVER_SEATBELT=index % 2)),
                packed("DOORS", 0, dict(FL=int(index % 31 == 0), FR=int(index % 32 == 0), BL=int(index % 33 == 0), BR=int(index % 34 == 0))),
                packed("CRZ_CTRL", 0, dict(CRZ_AVAILABLE=index % 2, CRZ_ACTIVE=int(index % 140 < 105))),
                packed("CRZ_EVENTS", 0, dict(CRZ_SPEED=(29, 31, 33, 65, 157, 160, 165)[(index // 15) % 7])),
                packed("CAM_LANEINFO", 2, dict(LANE_LINES=index % 5, LINE_VISIBLE=index % 2, LINE_NOT_VISIBLE=(index + 1) % 2, BIT1=index % 2, BIT2=(index // 2) % 2, BIT3=(index // 3) % 2, NO_ERR_BIT=index % 2, S1=index % 2, S1_HBEAM=(index // 4) % 2, HANDS_ON_STEER_WARN=index % 2, LDW_WARN_RL=1, LDW_WARN_LL=1)),
                packed("CAM_LKAS", 2, dict(CTR=index % 16, BIT_1=index % 2, ERR_BIT_1=int(index % 91 > 80), ERR_BIT_2=int(index % 71 > 60), LKAS_REQUEST=index % 800, STEERING_ANGLE=index % 100)),
            ]
            if 180 <= index < 240:
                frames = [frame for frame in frames if frame["address"] != packer.dbc.name_to_msg["CAM_LANEINFO"].address]
            packets = [] if index % 71 == 70 else [dict(mono_time=now, frames=frames)]
            if 330 <= index < 390:
                packets = [dict(mono_time=now, frames=[])]
            control = car.CarControl.new_message(enabled=index % 130 < 110, latActive=index % 90 < 80, longActive=True, cruiseControl=dict(cancel=index % 100 < 13, resume=index % 80 > 70), hudControl=dict(setSpeed=(25, 50, 70, 100, 170)[(index // 17) % 5] / (3.6 if metric else 1. / (1.609344 * (1. / 3.6))), leadVisible=index % 2 == 0, visualAlert=("none", "steerRequired", "ldw")[(index // 50) % 3]), actuators=dict(torque=rng.uniform(-1.4, 1.4), accel=rng.uniform(-3., 2.), steeringAngleDeg=math.sin(index) * 50))
            if index in (40, 41):control.actuators.torque = 1e20 if index == 40 else -1e20
            if index in (60, 80):control.hudControl.setSpeed = 1e20 if index == 60 else -1e20
            setting = ("1", "0", "-1", "2", " +1suffix", "0xFF", " -2tail")[(index // 49) % 7]
            steps.append(dict(now=now, packets=packets, control=list(control.to_bytes()), settings={"SpeedFromPCM": setting}, is_metric=metric, soft_hold=index % 4, commit={"vCruise": 70., "activateCruise": index % 2}))
        result.append(dict(name=f"runtime-{profile}-{candidate}-{metric}", op="runtime", candidate=candidate, alpha_long=True, fingerprints=[], firmware=[], settings={"NNFF": "0", "NNFFLite": "0"}, now=2_000_000_000, steps=steps))
    return result
