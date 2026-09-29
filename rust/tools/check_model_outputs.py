# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.4.6", "pycapnp==2.1.0"]
# ///
# Run with PYTHONPATH=. uv run rust/tools/check_model_outputs.py --help
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

import numpy as np

from openpilot.cereal import log
from openpilot.selfdrive.modeld import fill_model_msg
from openpilot.selfdrive.modeld.parse_model_outputs import Parser
from model_output_fixtures import OutputFixture, captured, synthetic
from model_output_reference import compare, new_message, original_functions


def check_exp(destination: Path, binary: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    random = np.random.default_rng(20260930)
    values = random.uniform(-105, 12, 250000).astype(np.float32)
    edges = np.array([-np.inf, np.inf, np.nan, -3e-8, -4e-8, -5e-8, 0, 11], dtype=np.float32)
    values = np.r_[values, edges]
    expected = np.exp(np.clip(values, -np.inf, 11))
    values.tofile(destination / "input.bin")
    subprocess.run([binary, destination / "input.bin", destination / "output.bin"], check=True)
    actual = np.fromfile(destination / "output.bin", dtype=np.float32)
    assert actual.shape == expected.shape
    equivalent = (actual.view(np.uint32) == expected.view(np.uint32)) | (np.isnan(actual) & np.isnan(expected))
    assert np.all(equivalent), (values[~equivalent][:8], expected[~equivalent][:8], actual[~equivalent][:8])
    (destination / "report.json").write_text(json.dumps({"float32_values": values.size, "comparison": "exact bits, equivalent NaNs",
                                                        "numpy": np.__version__, "device_validation": False}, indent=2) + "\n")


def check(fixture: OutputFixture, destination: Path, binary: Path) -> int:
    destination.mkdir(parents=True, exist_ok=False)
    action_module, driver_module = original_functions()
    state = fill_model_msg.PublishState()
    previous = log.ModelDataV2.Action.new_message()
    frames = []
    expected = []
    for index, values in enumerate(fixture.frames):
        data = values.tobytes()
        filename = f"input-{index}.bin"
        (destination / filename).write_bytes(data)
        raw = index % 2 == 0
        outputs = {name: values[None, start:end].copy() for name, (start, end) in fixture.slices.items()}
        now = 1000000000 + 50000000 * index
        if fixture.kind == "driver":
            outputs = driver_module.parse_model_output(outputs)
            outputs["raw_pred"] = data if raw else b""
            reference = driver_module.get_driverstate_packet(outputs, index, now, 0.031, 0.022)
            reference.logMonoTime = now
            expected.append(reference.to_dict())
            frames.append({"kind": "driver", "input": filename, "raw": raw,
                               "timing": {"log_mono_time": now, "frame_id": index, "model_execution_time": 0.031, "gpu_execution_time": 0.022}})
        else:
            outputs = Parser().parse_outputs(outputs)
            outputs["raw_pred"] = values.copy()
            speed = [0.0, 0.3, 0.3001, 1.0, 17.3][index % 5]
            action_inputs = {"lat_action_t": 0.2, "long_action_t": 0.3, "v_ego": speed, "lat_smooth_seconds": 0.15, "v_ego_stopping": 0.05}
            action = action_module.get_action_from_model(outputs, previous, 0.2, 0.3, speed, 0.15, 0.05)
            previous = action
            frame = {"log_mono_time": now, "frame_id": index, "frame_id_extra": index + 1,
                         "camera_state_frame_id": max(0, index - 2) if index % 2 else index + 3,
                         "frame_drop": 0.125, "timestamp_eof": now - 10000000, "model_execution_time": 0.043, "valid": index % 3 != 0}
            pose = {"log_mono_time": now, "frame_id": index, "dropped_frames": index % 4,
                    "timestamp_eof": now - 8000000, "live_calibration_seen": index % 3 != 0}
            model = new_message("modelV2")
            model.logMonoTime = now
            fill_model_msg.SEND_RAW_PRED = raw
            fill_model_msg.fill_model_msg(model, outputs, action, state, index, index + 1, frame["camera_state_frame_id"], 0.125,
                                          frame["timestamp_eof"], 0.043, frame["valid"])
            model.modelV2.meta.laneChangeState = index % 4
            model.modelV2.meta.laneChangeDirection = index % 3
            driving = new_message("drivingModelData")
            driving.logMonoTime = now
            fill_model_msg.fill_driving_model_data(driving, model)
            odometry = new_message("cameraOdometry")
            odometry.logMonoTime = now
            fill_model_msg.fill_pose_msg(odometry, outputs, index, pose["dropped_frames"], pose["timestamp_eof"], pose["live_calibration_seen"])
            expected.extend(message.to_dict() for message in (model, driving, odometry))
            frames.append({"kind": "driving", "input": filename, "timing": frame, "pose": pose, "action": action_inputs,
                               "lane_change_state": index % 4, "lane_change_direction": index % 3, "raw": raw})
    request = destination / "request.json"
    request.write_text(json.dumps({"slices": fixture.slices, "frames": frames}))
    subprocess.run([binary, request, destination / "rust"], check=True)
    messages = list(log.Event.read_multiple_bytes((destination / "rust/messages.bin").read_bytes()))
    assert len(messages) == len(expected), (len(messages), len(expected))
    compared = sum(compare(reference, actual.to_dict(), f"{fixture.kind}[{i}]") for i, (reference, actual) in enumerate(zip(expected, messages, strict=True)))
    (destination / "report.json").write_text(json.dumps({"frames": len(frames), "messages": len(messages), "compared_fields": compared,
                                                           "probability_tolerance": 1e-6, "polynomial_tolerance": 2e-5,
                                                           "discrete_fields": "exact", "device_validation": False}, indent=2) + "\n")
    return compared


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare Rust model publications with the original Python implementations")
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--driving-pipeline", type=Path)
    parser.add_argument("--driving-outputs", type=Path)
    parser.add_argument("--driver-pipeline", type=Path)
    parser.add_argument("--driver-outputs", type=Path)
    args = parser.parse_args()
    check_exp(args.output.resolve() / "exp", args.binary.resolve().with_name("exp_probe"))
    count = 0
    for name, fixture in (("driving", synthetic("driving")), ("mixture", synthetic("driving", True)), ("driver", synthetic("driver"))):
        count += check(fixture, args.output.resolve() / name, args.binary.resolve())
    for kind, pipeline, outputs in (("driving", args.driving_pipeline, args.driving_outputs), ("driver", args.driver_pipeline, args.driver_outputs)):
        if (pipeline is None) != (outputs is None):
            parser.error(f"both --{kind}-pipeline and --{kind}-outputs are required together")
        if pipeline is not None:
            for fixture in captured(pipeline, outputs, kind):
                count += check(fixture, args.output.resolve() / f"captured-{kind}", args.binary.resolve())
    print(json.dumps({"compared_fields": count, "status": "passed", "device_validation": False}))


if __name__ == "__main__":
    main()
