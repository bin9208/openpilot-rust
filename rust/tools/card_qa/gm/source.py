from __future__ import annotations

import contextlib
import copy
import io
from can_source import load
from card_qa.mazda.source import Settings as BaseSettings, LogCapture


class Settings(BaseSettings):
  def get_float(self, key: str) -> float:
    import numpy as np
    return float(np.float32(self.values.get(key, 0)))

  def put_bool_nonblocking(self, key: str, value: bool) -> None:
    self.put_nonblocking(key, "1" if value else "0")


def extras(vehicle):
  names = ("loopback_lka_steering_cmd_updated", "loopback_lka_steering_cmd_ts_nanos", "pt_lka_steering_cmd_counter", "cam_lka_steering_cmd_counter", "buttons_counter", "single_pedal_mode", "pedal_steady", "cruise_buttons", "distance_button", "cruiseMain_on", "lkas_enabled", "pscm_status", "lkas_status", "pcm_acc_status")
  return {key: copy.deepcopy(getattr(vehicle.CS, key, None)) for key in names}


def controller(vehicle):
  names = ("start_time", "apply_torque_last", "apply_gas", "apply_brake", "apply_speed", "frame", "last_steer_frame", "last_button_frame", "cancel_counter", "pedal_steady", "lka_steering_cmd_counter", "lka_icon_status_last", "long_pitch", "use_ev_tables", "accel_g", "activateCruise_after_brake")
  state = {key: getattr(vehicle.CC, key) for key in names}
  state["lka_icon_status_last"] = list(state["lka_icon_status_last"])
  state["pitch"] = vehicle.CC.pitch.x
  limits = {key.lower(): copy.deepcopy(getattr(vehicle.CC.params, key, None)) for key in ("STEER_MAX", "STEER_DELTA_UP", "STEER_DELTA_DOWN", "MAX_GAS", "MAX_ACC_REGEN", "INACTIVE_REGEN", "GAS_LOOKUP_BP", "GAS_LOOKUP_V", "BRAKE_LOOKUP_BP", "EV_GAS_LOOKUP_BP", "EV_BRAKE_LOOKUP_BP")}
  return {"state": state, "limits": limits}


def counters(vehicle):
  return [{str(k): str(v) for k, v in packer.counters.items()} for packer in (vehicle.CC.packer_pt, vehicle.CC.packer_obj, vehicle.CC.packer_ch)]


def state_snapshot(vehicle):
  return {"speed_filter": [float(row[0]) for row in vehicle.CS.v_ego_kf.x], "cluster_filter": [float(row[0]) for row in vehicle.CS.v_ego_clu_kf.x], "cluster_seen": vehicle.v_ego_cluster_seen}


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.gm.interface import CarInterface
  from opendbc.can import parser
  from opendbc.car.carlog import carlog

  settings = Settings(case["settings"])
  from opendbc.car.gm import interface, carstate, carcontroller
  from openpilot.selfdrive.car import cruise
  interfaces.Params = interface.Params = carstate.Params = carcontroller.Params = cruise.Params = lambda *args: settings
  now = case["now"]
  parser.time = type("Clock", (), {"monotonic_ns": staticmethod(lambda: now)})
  firmware = [structs.CarParams.CarFw.new_message(ecu=fw["ecu"], fwVersion=bytes(fw["fw_version"]), request=[bytes(r) for r in fw.get("request", [])]) for fw in case["firmware"]]
  fingerprint = {bus: {} for bus in range(8)}
  fingerprint.update({bus: dict(rows) for bus, rows in case["fingerprints"]})
  parameter_prints = io.StringIO()
  parameter_logs = LogCapture()
  carlog.addHandler(parameter_logs)
  with contextlib.redirect_stdout(parameter_prints):
    try:
      cp = CarInterface.get_params(case["candidate"], fingerprint, firmware, case["alpha_long"], True, False)
    except KeyError as error:
      carlog.removeHandler(parameter_logs)
      assert error.args == (case["candidate"],)
      return {"error": {"kind": "MissingTorque", "candidate": case["candidate"]}, "writes": settings.writes}
  carlog.removeHandler(parameter_logs)
  result = {"parameter_prints": parameter_prints.getvalue().splitlines(), "parameter_logs": parameter_logs.rows[:]}
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
  result["initial_packer_counters"] = counters(vehicle)
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
        except AttributeError as error:
          pre_update_error = {"kind": type(error).__name__, "message": str(error)}
        else:
          raise AssertionError("initial GM apply unexpectedly succeeded")
      break
    settings.values.update(step["settings"])
    now = step["now"]
    packets = [(packet["mono_time"], [(frame["address"], bytes(frame["data"]), frame["bus"]) for frame in packet["frames"]]) for packet in step["packets"]]
    logs.rows.clear()
    write_start = len(settings.writes)
    try:
      state = vehicle.update(packets)
    except AssertionError as error:
      if case["op"] != "state_error":
        raise
      pre_update_error = {"kind": type(error).__name__, "message": str(error)}
      break
    vehicle.CS.softHoldActive = step["soft_hold"]
    vehicle.CS.is_metric = step["is_metric"]
    for key, value in step["commit"].items():
      setattr(state, key, value)
    vehicle.CS.out = state
    with structs.CarControl.from_bytes(bytes(step["control"])) as control, contextlib.redirect_stdout(captured):
      try:
        actuators, can = vehicle.apply(control, now)
      except ValueError as error:
        if case["op"] != "numeric_error":
          raise
        pre_update_error = {"kind": type(error).__name__, "message": str(error)}
        break
    steps.append(
      {
        "state": list(state.to_bytes()),
        "actuators": list(actuators.to_bytes()),
        "can": [{"address": address, "data": list(data), "bus": bus} for address, data, bus in can],
        "extra": extras(vehicle),
        "state_snapshot": state_snapshot(vehicle),
        "controller": controller(vehicle),
        "packer_counters": counters(vehicle),
        "logs": logs.rows[:],
        "writes": settings.writes[write_start:],
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
    "final_packer_counters": counters(vehicle),
  }
