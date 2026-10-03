from __future__ import annotations

import contextlib
import copy
import io
from can_source import load
from card_qa.mazda.source import Settings, LogCapture


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.nissan.interface import CarInterface
  from opendbc.can import parser
  from opendbc.car.carlog import carlog

  settings = Settings(case["settings"])
  interfaces.Params = lambda: settings
  now = case["now"]
  parser.time = type("Clock", (), {"monotonic_ns": staticmethod(lambda: now)})
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw["ecu"], fwVersion=bytes(fw["fw_version"])) for fw in case["firmware"]]
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in case["fingerprints"]})
  parameter_prints = io.StringIO()
  with contextlib.redirect_stdout(parameter_prints):
    cp = CarInterface.get_params(case["candidate"], fingerprint, firmware, case["alpha_long"], True, False)
  result = {"parameter_prints": parameter_prints.getvalue().splitlines()}
  if case["op"] == "params":
    return dict(**result, params=list(cp.to_bytes()), writes=settings.writes)
  cp.carFw = firmware
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured):
    vehicle = CarInterface(cp)
  result["common"] = {"use_nnff": vehicle.use_nnff, "use_nnff_lite": vehicle.use_nnff_lite, "model_present": vehicle.lat_torque_nn_model is not None}
  logs = LogCapture()
  carlog.addHandler(logs)
  calls = []

  def recv(*args):
    calls.append(["recv"])
    return []

  def send(*args):
    calls.append(["send"])

  vehicle.init(cp, recv, send)
  steps = []
  for step in case["steps"]:
    settings.values.update(step["settings"])
    now = step["now"]
    packets = [(packet["mono_time"], [(frame["address"], bytes(frame["data"]), frame["bus"]) for frame in packet["frames"]]) for packet in step["packets"]]
    logs.rows.clear()
    state = vehicle.update(packets)
    vehicle.CS.softHoldActive = step["soft_hold"]
    vehicle.CS.is_metric = step["is_metric"]
    for key, value in step["commit"].items():
      setattr(state, key, value)
    vehicle.CS.out = state
    with structs.CarControl.from_bytes(bytes(step["control"])) as control, contextlib.redirect_stdout(captured):
      actuators, can = vehicle.apply(control, now)
    extras = {
      key: copy.deepcopy(getattr(vehicle.CS, key, {})) for key in ("distance_button", "cruise_throttle_msg", "cancel_msg", "lkas_hud_msg", "lkas_hud_info_msg")
    }
    extras["steeringTorqueSamples"] = list(vehicle.CS.steeringTorqueSamples)
    controller = {key: getattr(vehicle.CC, key) for key in ("frame", "apply_angle_last")}
    steps.append(
      {
        "state": list(state.to_bytes()),
        "actuators": list(actuators.to_bytes()),
        "can": [{"address": address, "data": list(data), "bus": bus} for address, data, bus in can],
        "extra": extras,
        "controller": controller,
        "logs": logs.rows[:],
        "soft_hold": vehicle.CS.softHoldActive,
        "is_metric": vehicle.CS.is_metric,
      }
    )
  vehicle.deinit(cp, recv, send)
  carlog.removeHandler(logs)
  return dict(**result, params=list(cp.to_bytes()), writes=settings.writes, steps=steps, lifecycle=calls, prints=captured.getvalue().splitlines())
