from __future__ import annotations

import contextlib
import copy
import io
from can_source import load
from card_qa.mazda.source import Settings, LogCapture


def extras(vehicle):
  names = ("cruise_button", "ready", "es_distance_msg", "es_dashstatus_msg", "es_lkas_state_msg",
           "es_brake_msg", "es_status_msg", "cruise_control_msg", "es_infotainment_msg")
  result = {key: copy.deepcopy(getattr(vehicle.CS, key, None)) for key in names}
  result.update(left_blinker_cnt=vehicle.CS.left_blinker_cnt, right_blinker_cnt=vehicle.CS.right_blinker_cnt,
                angle_previous_value=vehicle.CS.angle_rate_calulator.previous_value, angle_rate=vehicle.CS.angle_rate_calulator.rate)
  return result


def controller(vehicle):
  names = ("frame", "apply_torque_last", "cruise_button_prev", "steer_rate_counter")
  return {key: getattr(vehicle.CC, key) for key in names}


def state_snapshot(vehicle):
  return {"speed_filter": [float(row[0]) for row in vehicle.CS.v_ego_kf.x], "cluster_seen": vehicle.v_ego_cluster_seen}


def trace(case):
  load()
  from opendbc.car import interfaces, structs
  from opendbc.car.subaru.interface import CarInterface
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
  cp.flags |= case.get('flags_or', 0)
  if case.get('force_long', False):
    cp.openpilotLongitudinalControl = True
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
  incoming = []
  sent = []

  def recv(*args, **kwargs):
    calls.append(["recv"])
    result = incoming[:]
    incoming.clear()
    return result

  def send(frames):
    from opendbc.car.can_definitions import CanData
    calls.append(["send"])
    for address, data, bus in frames:
      sent.append({'address': address, 'data': list(data), 'bus': bus})
      assert address == 0x787 and bus == 2
      if data[:3] == b'\x02\x10\x03':
        incoming.append([CanData(0x78f, b'\x02\x50\x03\x00\x00\x00\x00\x00', 2)])
      elif data[:4] == b'\x03\x28\x03\x01':
        incoming.append([CanData(0x78f, b'\x03\x68\x03\x01\x00\x00\x00\x00', 2)])
      else:
        raise AssertionError('unexpected Subaru lifecycle request')

  vehicle.init(cp, recv, send)
  lifecycle_logs = logs.rows[:]
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
          raise AssertionError("initial Subaru apply unexpectedly succeeded")
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
    "lifecycle_can": sent,
    "lifecycle_logs": lifecycle_logs,
    "prints": captured.getvalue().splitlines(),
    "pre_update_error": pre_update_error,
    "final_extra": extras(vehicle),
    "final_state_snapshot": state_snapshot(vehicle),
    "final_controller": controller(vehicle),
    "final_packer_counters": {str(k): str(v) for k, v in vehicle.CC.packer.counters.items()},
  }
