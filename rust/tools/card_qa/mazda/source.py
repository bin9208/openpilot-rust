from __future__ import annotations

import contextlib
import copy
import io
import logging
import re
from can_source import load


class Settings:
    def __init__(self, values: dict[str, str]) -> None:
        self.values = dict(values)
        self.writes = []

    def get_bool(self, key: str) -> bool:
        return self.values.get(key) == "1"

    def get_int(self, key: str) -> int:
        raw = self.values.get(key, "")
        if not raw:return 0
        match = re.match(r"\s*[+-]?\d+", raw, flags=re.ASCII)
        if match is None:raise ValueError("stoi")
        value = int(match[0])
        if not -(2**31) <= value < 2**31:raise OverflowError("stoi")
        return value

    def put_int(self, key: str, value: int) -> None:
        self.values[key] = str(value)
        self.writes.append([key, str(value)])

    def put_nonblocking(self, key: str, value: str) -> None:
        self.values[key] = value
        self.writes.append([key, value])


class LogCapture(logging.Handler):
    def __init__(self) -> None:
        super().__init__()
        self.rows = []

    def emit(self, record: logging.LogRecord) -> None:
        self.rows.append([record.levelname, record.getMessage()])


def trace(case):
    load()
    from opendbc.car import interfaces, structs
    from opendbc.car.mazda import carcontroller
    from opendbc.car.mazda.interface import CarInterface
    from opendbc.can import parser
    from opendbc.car.carlog import carlog
    settings = Settings(case["settings"])
    interfaces.Params = carcontroller.Params = lambda: settings
    now = case["now"]
    parser.time = type("Clock", (), {"monotonic_ns": staticmethod(lambda: now)})
    firmware = [structs.CarParams.CarFw.new_message(ecu=fw["ecu"], fwVersion=bytes(fw["fw_version"])) for fw in case["firmware"]]
    fingerprint = {bus: {} for bus in range(8)}
    fingerprint.update({bus: dict(rows) for bus, rows in case["fingerprints"]})
    parameter_prints = io.StringIO()
    with contextlib.redirect_stdout(parameter_prints):
        cp = CarInterface.get_params(case["candidate"], fingerprint, firmware, case["alpha_long"], True, False)
    result = dict(parameter_prints=parameter_prints.getvalue().splitlines())
    if case["op"] == "params":
        return dict(**result, params=list(cp.to_bytes()), writes=settings.writes)
    cp.carFw = firmware
    captured = io.StringIO()
    with contextlib.redirect_stdout(captured):
        vehicle = CarInterface(cp)
    result["common"] = dict(use_nnff=vehicle.use_nnff, use_nnff_lite=vehicle.use_nnff_lite, model_present=vehicle.lat_torque_nn_model is not None)
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
        keys = ("crz_btns_counter", "acc_active_last", "low_speed_alert", "lkas_allowed_speed", "lkas_disabled", "prev_distance_button", "distance_button", "prev_cruise_buttons", "cruise_buttons", "lkas_previously_enabled", "lkas_enabled", "left_blinker_cnt", "right_blinker_cnt", "cam_lkas", "cam_laneinfo")
        extras = {key: copy.deepcopy(getattr(vehicle.CS, key)) for key in keys}
        snapshot = {key: getattr(vehicle.CC, key) for key in ("frame", "apply_torque_last", "brake_counter", "activateCruise", "speed_from_pcm")}
        steps.append(dict(state=list(state.to_bytes()), actuators=list(actuators.to_bytes()), can=[dict(address=address, data=list(data), bus=bus) for address, data, bus in can], extra=extras, controller=snapshot, logs=logs.rows[:], soft_hold=vehicle.CS.softHoldActive, is_metric=vehicle.CS.is_metric))
    vehicle.deinit(cp, recv, send)
    carlog.removeHandler(logs)
    return dict(**result, params=list(cp.to_bytes()), writes=settings.writes, steps=steps, lifecycle=calls, prints=captured.getvalue().splitlines())
