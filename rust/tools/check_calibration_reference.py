#!/usr/bin/env python3
"""Compare every estimator step and complete packet against the actual calibration class."""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

import numpy as np

from calibration_cases import histories
from calibration_reference import Parameters, close, compare_packet, compare_state, original
from openpilot.cereal import log


def check(binary: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    largest = float(np.finfo(np.float32).max)
    assert close(largest, largest, wire=True)
    assert not close(0., largest, wire=True) and not close(float("inf"), largest, wire=True)
    scenarios, statuses = Counter(), Counter()
    fields = failures = writes = steps = 0
    calibrator = None
    with subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1) as process, \
         (destination / "trace.jsonl").open("w") as trace:
        assert process.stdin is not None and process.stdout is not None
        for category, command in histories():
            process.stdin.write(json.dumps(command) + "\n")
            process.stdin.flush()
            actual = json.loads(process.stdout.readline())
            expected_error = None
            accepted = None
            persisted = False
            try:
                if command["action"] == "reset":
                    params = Parameters(bytes(command["saved"]) if command["saved"] is not None else None)
                    source = original(command["mici"], params)
                    calibrator = source.Calibrator(param_put=True)
                    if command["seed"] is not None:
                        seed = command["seed"]
                        calibrator.reset(np.array(list(map(float, seed["rpy"]))), seed["valid_blocks"],
                                         np.array(list(map(float, seed["wide"]))), np.array(list(map(float, seed["height"]))))
                        calibrator.update_status()
                    calibrator.not_car = command["not_car"]
                else:
                    calibrator.handle_v_ego(float(command["v_ego"]))
                    input_values = {name: list(map(float, value)) for name, value in command["input"].items()}
                    before = len(params.writes)
                    if not (abs(float(command["trim"])) > 1e-6 and calibrator.cal_status == log.LiveCalibrationData.Status.calibrated):
                        accepted = calibrator.handle_cam_odom(input_values["trans"], input_values["rot"], input_values["wide"],
                                                              input_values["trans_std"], input_values["road"], input_values["road_std"])
                    persisted = len(params.writes) != before
                wanted = calibrator.get_msg(command.get("valid", True))
                wanted.logMonoTime = command.get("timestamp", 0)
            except (ValueError, IndexError, TypeError) as error:
                expected_error = type(error).__name__
            try:
                if expected_error:
                    assert "error" in actual, (expected_error, actual)
                    failures += 1
                else:
                    assert "error" not in actual, actual
                    compare_state(actual, calibrator)
                    with log.Event.from_bytes(bytes(actual["packet"])) as event:
                        fields += compare_packet(event.to_dict(), wanted.to_dict())
                    if command["action"] == "update":
                        assert actual["persist"] == persisted
                        assert (actual["accepted"] is None) == (accepted is None)
                        if accepted is not None:
                            assert all(close(float(a), b) for a, b in zip(actual["accepted"], accepted, strict=True))
                        writes += int(persisted)
                    statuses[actual["status"]] += 1
            except AssertionError:
                (destination / "failure.json").write_text(json.dumps({"category": category, "step": steps, "command": command,
                                                                      "actual": actual, "expected_error": expected_error}, indent=2) + "\n")
                raise
            scenarios[category] += 1
            steps += 1
            trace.write(json.dumps({"scenario": category, "step": steps, "state": actual}) + "\n")
        process.stdin.close()
        assert process.wait(timeout=5) == 0
    assert statuses["recalibrating"] and statuses["invalid"] and statuses["calibrated"]
    assert writes >= 10 and failures >= 10
    report = {"result": "pass", "steps": steps, "packet_fields": fields, "expected_source_errors": failures,
              "persistence_steps": writes, "scenarios": scenarios, "statuses": statuses,
              "state_tolerance": {"absolute": 2e-12, "relative": 2e-12},
              "wire_tolerance": "one Float32 ULP or absolute2e-12 cancellation floor; discrete and nonfinite classes exact"}
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    check(args.binary.resolve(), args.output.resolve())
