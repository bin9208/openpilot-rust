from __future__ import annotations

import contextlib
import copy
import io
from can_source import load
from card_qa.mazda.source import Settings, LogCapture


def extras(vehicle):
  names = ("last_speed", "steering_pressed_cnt", "acm_lka_hba_cmd", "sccm_wheel_touch", "vdm_adas_status")
  return {key: copy.deepcopy(getattr(vehicle.CS, key)) for key in names}


def controller(vehicle):
  names = ("frame", "apply_torque_last", "cancel_frames")
  return {key: getattr(vehicle.CC, key) for key in names}


def state_snapshot(vehicle):
  return {"speed_filter": [float(row[0]) for row in vehicle.CS.v_ego_kf.x], "cluster_seen": vehicle.v_ego_cluster_seen}


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.rivian.interface import CarInterface
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
    return {**result, "params": list(cp.to_bytes()), "writes": settings.writes}
  cp.carFw = firmware
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured):
    vehicle = CarInterface(cp)
  result["common"] = {"use_nnff": vehicle.use_nnff, "use_nnff_lite": vehicle.use_nnff_lite, "model_present": vehicle.lat_torque_nn_model is not None}
  result["initial_state"] = list(vehicle.CS.out.to_bytes())
  result["initial_extra"] = extras(vehicle)
  result["initial_state_snapshot"] = state_snapshot(vehicle)
  result["initial_controller"] = controller(vehicle)
  result["initial_packer_counters"] = {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()}
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
  pre_update_error = None
  for step in case["steps"]:
    if case["op"] == "before_update":
      with structs.CarControl.from_bytes(bytes(step["control"])) as control, contextlib.redirect_stdout(captured):
        try:
          vehicle.apply(control, now)
        except TypeError as error:
          pre_update_error = {"kind": type(error).__name__, "message": str(error)}
        else:
          raise AssertionError("initial Rivian apply unexpectedly succeeded")
      break
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
    steps.append(
      {
        "state": list(state.to_bytes()),
        "actuators": list(actuators.to_bytes()),
        "can": [{"address": address, "data": list(data), "bus": bus} for address, data, bus in can],
        "extra": extras(vehicle),
        "state_snapshot": state_snapshot(vehicle),
        "controller": controller(vehicle),
        "packer_counters": {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()},
        "logs": logs.rows[:],
        "soft_hold": vehicle.CS.softHoldActive,
        "is_metric": vehicle.CS.is_metric,
      }
    )
  vehicle.deinit(cp, recv, send)
  carlog.removeHandler(logs)
  return {
    **result,
    "params": list(cp.to_bytes()),
    "writes": settings.writes,
    "steps": steps,
    "lifecycle": calls,
    "prints": captured.getvalue().splitlines(),
    "pre_update_error": pre_update_error,
    "final_extra": extras(vehicle),
    "final_state_snapshot": state_snapshot(vehicle),
    "final_controller": controller(vehicle),
    "final_packer_counters": {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()},
  }
