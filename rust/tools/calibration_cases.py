"""Deterministic calibration histories, source thresholds and malformed-array cases."""
from __future__ import annotations

import numpy as np

from calibration_reference import CV
from openpilot.cereal import log


def reset(*, mici=False, seed=None, saved=None, not_car=False) -> dict:
    return {"action": "reset", "mici": mici, "seed": seed, "saved": saved, "not_car": not_car}


def seed(**overrides) -> dict:
    values = {"rpy": [0.0, 0.0, 0.0], "wide": [0.0, 0.0, 0.0], "height": [1.22], "valid_blocks": 0, **overrides}
    return {name: value if name == "valid_blocks" else [str(float(v)) for v in value] for name, value in values.items()}


def update(index: int, **overrides) -> dict:
    inputs = {"trans": [10., 0., 0.], "rot": [0., 0., 0.], "trans_std": [0., 0., 0.],
              "wide": [0.01, 0.02, 0.03], "road": [0., 0., 1.5], "road_std": [0., 0., 0.]}
    inputs.update({key: value for key, value in overrides.items() if key in inputs})
    return {"action": "update", "timestamp": index + 1, "valid": index % 7 != 0,
            "v_ego": str(overrides.get("v_ego", 10.0)), "trim": str(overrides.get("trim", 0.0)),
            "input": {key: [str(float(v)) for v in value] for key, value in inputs.items()}}


def histories():
    for mici in (False, True):
        yield "long-history", reset(mici=mici)
        for index in range(6200):
            yaw, pitch = 0.0005 * np.sin(index / 200), 0.0003 * np.cos(index / 113)
            yield "long-history", update(index, trans=[10., 10. * np.tan(yaw), -10. * np.tan(pitch)],
                                          wide=[pitch, yaw, 0.1], road=[0., 0., 1.22 + 0.2 * np.sin(index / 110)])
        yield "spread-reset", reset(mici=mici)
        for index in range(1100):
            change = 0.05 if 500 <= index < 600 else 0.0
            yield "spread-reset", update(index, trans=[10., 10. * np.tan(change), 0.], v_ego=0.0 if 600 <= index < 612 else 10.)
    for blocks in (0, 5):
        speed, yaw_rate, angle_std, height_std = 15 * CV.MPH_TO_MS, np.radians(2), np.radians(0.25), np.exp(-3.5)
        for value in (np.nextafter(speed, -np.inf), speed, np.nextafter(speed, np.inf)):
            for key in ("v_ego", "trans"):
                yield "speed-boundaries", reset(seed=seed(valid_blocks=blocks))
                yield "speed-boundaries", update(0, **{key: value if key == "v_ego" else [value, 0., 0.]})
        for value in (np.nextafter(yaw_rate, -np.inf), yaw_rate, np.nextafter(yaw_rate, np.inf)):
            yield "yaw-boundaries", reset(seed=seed(valid_blocks=blocks))
            yield "yaw-boundaries", update(0, rot=[0., 0., value])
        for value in (np.nextafter(angle_std, -np.inf), angle_std, np.nextafter(angle_std, np.inf)):
            yield "uncertainty-boundaries", reset(seed=seed(valid_blocks=blocks))
            yield "uncertainty-boundaries", update(0, trans_std=[0., 10. * np.tan(value), 0.])
        for value in (np.nextafter(height_std, -np.inf), height_std, np.nextafter(height_std, np.inf)):
            yield "height-boundaries", reset(seed=seed(valid_blocks=blocks))
            yield "height-boundaries", update(0, road_std=[0., 0., value])
        for value in (0., 1e-6, np.nextafter(1e-6, np.inf), -0.1, float("nan"), float("inf")):
            yield "freeze", reset(seed=seed(valid_blocks=blocks))
            yield "freeze", update(0, trim=value)
    for mici, limits in ((False, [-0.09074112085129739, 0.17]), (True, [-0.143101, 0.22235988])):
        for axis, bounds in ((1, limits), (2, [-0.06912048084718224, 0.06912048084718235])):
            for bound in bounds:
                for value in (np.nextafter(bound, -np.inf), bound, np.nextafter(bound, np.inf)):
                    rpy = [0., 0., 0.]
                    rpy[axis] = value
                    yield "validity-boundaries", reset(mici=mici, seed=seed(rpy=rpy, valid_blocks=5))
                    yield "validity-boundaries", update(0)
    for blocks in (-1, 0, 1, 2, 5, 50, 51):
        for rpy in ([], [0.], [0., 0.], [0., 0., 0., 0.], [float("nan")], [float("inf"), 0., 0.]):
            yield "saved-array-errors", reset(seed=seed(rpy=rpy, wide=[1.], height=[], valid_blocks=blocks))
            if blocks in (0, 1) and len(rpy) != 3 and all(np.isfinite(rpy)):
                yield "saved-array-errors", update(0)
    for name in ("trans", "rot", "trans_std", "wide", "road", "road_std"):
        for values in ([], [1.], [1., 2.], [0., 0., float("nan")], [float("inf")] * 3, [float("-inf")] * 3):
            yield "input-arrays", reset(seed=seed(valid_blocks=5))
            yield "input-arrays", update(0, **{name: values})
    for height in (float("nan"), float("inf")):
        yield "nonfinite-history", reset()
        for index in range(105):
            yield "nonfinite-history", update(index, wide=[float("nan"), float("inf"), float("-inf")], road=[0., 0., height])
    for roll in (np.pi, -np.pi, np.pi / 2, -np.pi / 2, 1e10):
        yield "rotations", reset(seed=seed(rpy=[roll, 0.01, -0.02], valid_blocks=5))
        yield "rotations", update(0, trans=[10., 0.02, 0.03])
    for not_car in (False, True):
        yield "not-car", reset(not_car=not_car)
        for index in range(20):
            yield "not-car", update(index, trim=0.2)
    for rpy, height, wide, blocks in (([0.01, 0.02, 0.03], [1.22], [0.01, 0.02, 0.03], 5),
                                       ([float("nan")], [], [1.], 0), ([], [], [], 5),
                                       ([0., 0., 0.], [float("nan")], [float("inf")] * 3, -5)):
        event = log.Event.new_message(valid=False)
        packet = event.init("liveCalibration")
        packet.rpyCalib, packet.height, packet.wideFromDeviceEuler, packet.validBlocks = rpy, height, wide, blocks
        yield "persisted-float32", reset(saved=list(event.to_bytes()))
    largest = float(np.finfo(np.float32).max)
    yield "finite-wire-limits", reset(seed=seed(wide=[largest, -largest, 0.], height=[largest], valid_blocks=5))
    yield "corrupt-cache", reset(saved=list(b"invalid cereal bytes"))
