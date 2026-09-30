"""Branch, retention, numerical and source-history scenarios for torque estimator QA."""

from __future__ import annotations
import numpy as np
from openpilot.cereal import car, log


def car_bytes(brand: str = "hyundai", tuning: str = "torque", factor: float = 2.0, friction: float = 0.12, fingerprint: str = "torque-qa") -> bytes:
  cp = car.CarParams.new_message()
  cp.brand, cp.carFingerprint = brand, fingerprint
  lateral = cp.lateralTuning.init(tuning)
  if tuning == "torque":
    lateral.latAccelFactor, lateral.friction = factor, friction
  return cp.to_bytes()


def new(**overrides) -> dict:
  return {"action": "new", "car": list(car_bytes()), "previous": None, "saved": None, "seed": 42, "decimated": False, "track_all": True, **overrides}


def points(per_bucket: int = 510, slope: float = 2.0) -> list[list[str]]:
  rng = np.random.default_rng(82910)
  return [[str(x), str(slope * x + 0.02 + rng.normal(0, 0.035))] for x in np.repeat([-0.4, -0.25, -0.15, -0.05, 0.05, 0.15, 0.25, 0.4], per_bucket)]


def packet(which: str, event_time: int, **fields) -> dict:
  event = log.Event.new_message(logMonoTime=event_time, valid=True)
  event.init(which)
  setattr(event, which, fields)
  return {"action": "event", "bytes": list(event.to_bytes())}


def histories():
  for seed in (0, 1, 42, 4294967295):
    yield "sampling", {"action": "sample", "seed": seed, "populations": [0, 1, 2, 8, 17, 2001, 12000, 8, 600], "count": 2000}
  for decimated in (False, True):
    for slope in (2.0, -2.0, 0.0, 1e-10, 1e8):
      yield "initial", new(decimated=decimated)
      yield "empty", {"action": "message", "valid": False, "points": True}
      yield "sparse", {"action": "add", "points": points(1, slope)}
      yield "sparse", {"action": "message", "valid": True, "points": True}
      yield "fitting", {"action": "add", "points": points(510, slope)}
      for i in range(7):
        yield "fitting", {"action": "message", "valid": i % 2 == 0, "points": i % 3 == 0}
  yield "retention", new()
  yield "retention", {"action": "add", "points": points(1550)}
  yield "retention", {"action": "message", "valid": True, "points": True}
  yield "boundaries", {"action": "add", "points": [[str(x), "0.3"] for x in [-np.inf, -0.5, -0.3, -0.2, -0.1, -0.0, 0.1, 0.2, 0.3, 0.5, np.inf, np.nan]]}
  yield "boundaries", {"action": "message", "valid": True, "points": True}
  for per_bucket in (1, 510):
    yield "nan_fit", new()
    yield "nan_fit", {"action": "add", "points": points(per_bucket)}
    yield "nan_fit", {"action": "add", "points": [["-0.4", "NaN"]] * 1500}
    yield "nan_fit", {"action": "message", "valid": True, "points": True}
  for brand, tuning in (("ford", "torque"), ("hyundai", "pid"), ("toyota", "torque")):
    yield "car_identity", new(car=list(car_bytes(brand, tuning)))
    yield "car_identity", {"action": "message", "valid": True, "points": True}
  cp = car_bytes()
  for mode in (
    "valid",
    "invalid",
    "version",
    "fingerprint",
    "tuning",
    "factor",
    "friction",
    "brand",
    "partial",
    "corrupt",
    "empty",
    "no_previous",
    "negative_decay",
    "nan_decay",
  ):
    event = log.Event.new_message()
    cache = event.init("liveTorqueParameters")
    cache.version, cache.liveValid = 1, mode != "invalid"
    cache.latAccelFactorFiltered, cache.latAccelOffsetFiltered, cache.frictionCoefficientFiltered = 1.8, -0.04, 0.1
    cache.decay = float("nan") if mode == "nan_decay" else -1.0 if mode == "negative_decay" else 90.0
    cache.points = [[-0.4, 0.3], [-0.25, -0.2]] + ([[0.1]] if mode == "partial" else [])
    previous = cp
    match mode:
      case "version":
        cache.version = 2
      case "fingerprint":
        previous = car_bytes(fingerprint="different")
      case "tuning":
        previous = car_bytes(tuning="pid")
      case "factor":
        previous = car_bytes(factor=2.1)
      case "friction":
        previous = car_bytes(friction=0.13)
      case "brand":
        previous = car_bytes(brand="ford")
    saved = b"garbage" if mode == "corrupt" else b"" if mode == "empty" else event.to_bytes()
    yield "cache_" + mode, new(previous=None if mode == "no_previous" else list(previous), saved=list(saved))
    yield "cache_" + mode, {"action": "message", "valid": True, "points": True}
  yield "history", new()
  for index in range(360):
    time = 1_000_000_000 + index * 50_000_000
    lag = 0.2 if index < 150 else -0.03 if index < 200 else 0.0
    yield "history_delay", packet("liveDelay", time, lateralDelay=lag)
    yield "history_control", packet("carControl", time, latActive=not 170 <= index < 176)
    yield "history_output", packet("carOutput", time, actuatorsOutput={"torque": float(np.sin(index * 0.15) * 0.44)})
    yield "history_state", packet("carState", time, vEgo=14.9 if index % 37 == 0 else 20.0, steeringPressed=230 <= index < 233)
    if index % 19 == 0:
      yield "history_calib", packet("liveCalibration", time, rpyCalib=[0.03, -0.05, 0.06], calStatus="uncalibrated")
    yield (
      "history_pose",
      packet(
        "livePose",
        time + 9999,
        timestamp=time,
        orientationNED={"x": 0.02, "valid": index % 43 != 0},
        angularVelocityDevice={"x": 0.01, "y": -0.02, "z": 0.2 if index % 17 == 0 else float(np.cos(index * 0.1) * 0.04), "valid": True},
        inputsOK=True,
        sensorsOK=True,
        posenetOK=True,
      ),
    )
  yield "history", {"action": "message", "valid": True, "points": True}
  for calibration in ([], [0.0], [0.0, 0.0], [0.0, 0.0, 0.0, 0.0]):
    yield "malformed_calibration", packet("liveCalibration", 99, rpyCalib=calibration)
  yield "missing_history", new()
  for index in range(100):
    yield "missing_history", packet("carOutput", index * 50_000_000, actuatorsOutput={"torque": -0.25})
  yield (
    "missing_history",
    packet(
      "livePose",
      0,
      timestamp=5_000_000_000,
      orientationNED={"valid": True},
      angularVelocityDevice={"valid": True},
      inputsOK=True,
      sensorsOK=True,
      posenetOK=True,
    ),
  )
  for speed, torque, yaw in (
    (15.0, -0.25, 0.03125),
    (16.0, -0.02, 0.03125),
    (16.0, -float(np.nextafter(np.float32(0.02), np.float32(np.inf))), 0.03125),
    (16.0, -0.25, 0.0625),
    (16.0, -0.25, float(np.nextafter(np.float32(0.0625), np.float32(np.inf)))),
  ):
    yield "admission_boundary", new()
    for index in range(100):
      time = index * 50_000_000
      yield "admission_boundary", packet("carControl", time, latActive=True)
      yield "admission_boundary", packet("carOutput", time, actuatorsOutput={"torque": torque})
      yield "admission_boundary", packet("carState", time, vEgo=speed)
    yield (
      "admission_boundary",
      packet(
        "livePose",
        0,
        timestamp=5_000_000_000,
        orientationNED={"valid": True},
        angularVelocityDevice={"valid": True, "z": yaw},
        inputsOK=True,
        sensorsOK=True,
        posenetOK=True,
      ),
    )
    yield "admission_boundary", {"action": "message", "valid": True, "points": True}
