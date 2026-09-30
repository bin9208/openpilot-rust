from __future__ import annotations

import argparse
import ast
import itertools
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

import numpy as np

from openpilot.cereal import log
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.common.transformations.camera import DEVICE_CAMERAS
from openpilot.common.transformations.model import get_warp_matrix
from openpilot.selfdrive.modeld.camera_sync import receive_camera_pair


def source_nodes():
    root = Path(__file__).resolve().parents[2]
    path = root / "openpilot/selfdrive/modeld/modeld.py"
    tree = ast.parse(path.read_text())
    main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
    loop = next(node for node in main.body if isinstance(node, ast.While) and any(
        isinstance(child, ast.Assign) and isinstance(child.targets[0], ast.Tuple)
        and [ast.unparse(value) for value in child.targets[0].elts] == ["loop_start", "cpu_start"] for child in node.body))
    start = next(i for i, node in enumerate(loop.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "vipc_dropped_frames")
    end = next(i for i, node in enumerate(loop.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "prepare_only")
    drops = compile(ast.Module(body=loop.body[start:end + 1], type_ignores=[]), str(path), "exec")
    calibration = next(node for node in loop.body if isinstance(node, ast.If) and ast.unparse(node.test).startswith("sm.updated['liveCalibration']"))
    calibration = compile(ast.Module(body=[calibration], type_ignores=[]), str(path), "exec")
    model = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "ModelState")
    run = next(node for node in model.body if isinstance(node, ast.FunctionDef) and node.name == "run")
    assert [ast.unparse(node.targets[0]) for node in run.body[1:6]] == [
        "inputs['desire_pulse'][0]", "self.npy['desire'][:]", "self.prev_desire[:]",
        "self.npy['traffic_convention'][:]", "self.npy['action_t'][:]",
    ]
    pulse = compile(ast.Module(body=run.body[1:6], type_ignores=[]), str(path), "exec")
    helpers = ast.parse((root / "openpilot/selfdrive/modeld/helpers.py").read_text())
    select = next(node for node in helpers.body if isinstance(node, ast.FunctionDef) and node.name == "select_vision_streams")
    scope = {}
    module = ast.Module(body=[ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0), select], type_ignores=[])
    exec(compile(ast.fix_missing_locations(module), "helpers.py", "exec"), scope)
    return drops, calibration, pulse, scope["select_vision_streams"]


class Camera:
    def __init__(self, frames):
        self.frames = iter(frames)
        self.count = 0
        self.frame_id = self.timestamp_sof = self.timestamp_eof = 0

    def recv(self):
        self.count += 1
        value = next(self.frames, None)
        if value is None:
            return None
        self.frame_id, self.timestamp_sof, self.timestamp_eof = (value[name] for name in ("frame_id", "timestamp_sof", "timestamp_eof"))
        return value


class SubMaster(dict):
    pass


def fixtures():
    random = np.random.default_rng(20260930)
    pairs = []
    for index in range(200):
        streams = []
        for _ in range(2):
            stamp = 1_000_000_000
            frames = []
            for frame in range(100):
                stamp += int(random.choice([23_000_000, 77_000_000, 50_000_000, 100_000_000]))
                frames.append(None if random.random() < .05 else {"frame_id": frame, "timestamp_sof": stamp, "timestamp_eof": stamp + 1000})
            streams.append(frames)
        pairs.append({"main": streams[0], "extra": streams[1] if index % 5 else None, "calls": 40})
    for skew in [-20_000_001, -20_000_000, 0, 20_000_000, 20_000_001]:
        pairs.append({"main": [{"frame_id": 1, "timestamp_sof": 100_000_000, "timestamp_eof": 101_000_000}],
                          "extra": [{"frame_id": 2, "timestamp_sof": 100_000_000 + skew, "timestamp_eof": 101_000_000 + skew}], "calls": 2})
    frames = []
    frame_id = 0
    for index in range(2000):
        frame_id = 0 if index % 201 == 0 else frame_id + int(random.choice([0, 1, 1, 1, 2, 12]))
        frames.append({"frame_id": frame_id, "desire": int(random.integers(-2, 10)), "is_rhd": bool(index % 2),
                           "lateral_time": float(random.uniform(-.5, 2)), "longitudinal_time": float(random.uniform(-.5, 2)),
                           "features": None if index % 3 == 0 else random.normal(size=32).astype(np.float32).tolist(), "reset": index % 157 == 0})
    calibration = []
    cameras = list(DEVICE_CAMERAS) + [("invalid", "invalid")]
    for index in range(5000):
        device, sensor = cameras[index % len(cameras)]
        angles = random.uniform(-.5, .5, 3).astype(np.float32)
        if index % 17 == 0:
            angles = random.uniform(-120000, 120000, 3).astype(np.float32)
        if index % 113 == 0:
            angles[index % 3] = [np.nan, np.inf, -np.inf][index % 3]
        calibration.append({"updated": index % 7 != 0, "road_seen": index % 13 != 0, "device_seen": index % 19 != 0,
                                "rpy_bits": angles.view(np.uint32).tolist(), "calibrated": index % 2 == 0,
                                "yaw_trim_degrees": float(random.uniform(-4, 4)), "device": device, "sensor": sensor,
                                "main_wide": index % 3 == 0, "use_extra": index % 3 == 1})
    return {"feature_count": 32, "pairs": pairs, "streams": list(itertools.product([False, True], repeat=3)), "frames": frames, "calibration": calibration}


def check(request, actual):
    drops, calibration, pulse, select = source_nodes()
    for requested, result in zip(request["pairs"], actual["pairs"], strict=True):
        main = Camera(requested["main"])
        extra = Camera(requested["extra"]) if requested["extra"] is not None else None
        selected = []
        for _ in range(requested["calls"]):
            value = receive_camera_pair(main, extra)
            selected.append(None if value is None else [vars(value[1]), vars(value[3])])
        assert selected == result["selected"]
        assert (main.count, extra.count if extra else 0) == (result["main_receives"], result["extra_receives"])
    for requested, result in zip(request["streams"], actual["streams"], strict=True):
        road, wide, enabled = requested
        selected, use_extra = select([name for name, present in (("road", road), ("wide_road", wide)) if present], "road", "wide_road", enabled)
        assert result == ([selected, use_extra] if selected is not None else None)
    namespace = {"frame_dropped_filter": FirstOrderFilter(0.0, 10.0, .05), "last_vipc_frame_id": 0, "run_count": 0}
    inputs = SimpleNamespace(prev_desire=np.zeros(8, dtype=np.float32), npy={name:np.zeros(shape, dtype=np.float32)
                                for name, shape in (("desire", (8,)), ("traffic_convention", (1, 2)), ("action_t", (1, 2)))})
    features = np.zeros(32, dtype=np.float32)
    for frame, result in zip(request["frames"], actual["frames"], strict=True):
        namespace["meta_main"] = SimpleNamespace(frame_id=frame["frame_id"])
        if frame["reset"]:
            namespace["run_count"] = 0
        exec(drops, namespace)
        assert result["drop"]["dropped"] == namespace["vipc_dropped_frames"]
        assert result["drop"]["prepare_only"] == namespace["prepare_only"]
        assert abs(result["drop"]["ratio"] - namespace["frame_drop_ratio"]) < 1e-15
        namespace["last_vipc_frame_id"] = frame["frame_id"]
        desire = np.zeros(8, dtype=np.float32)
        if 0 <= frame["desire"] < 8:
            desire[frame["desire"]] = 1
        values = {"desire_pulse": desire, "traffic_convention": np.array([not frame["is_rhd"], frame["is_rhd"]]),
                      "action_t": np.array([frame["lateral_time"], frame["longitudinal_time"]], dtype=np.float32)}
        exec(pulse, {"np": np, "self": inputs, "inputs": values})
        packed = np.r_[inputs.npy["desire"].ravel(), inputs.npy["traffic_convention"].ravel(), inputs.npy["action_t"].ravel(), features]
        assert packed.view(np.uint32).tolist() == result["packed_bits"]
        if frame["features"] is not None:
            features[:] = frame["features"]
    namespace = {"np": np, "log": log, "DEVICE_CAMERAS": DEVICE_CAMERAS, "get_warp_matrix": get_warp_matrix,
                     "model_transform_main": np.zeros((3, 3), dtype=np.float32), "model_transform_extra": np.zeros((3, 3), dtype=np.float32),
                     "live_calib_seen": False}
    compared = exact = signed_zeros = 0
    maximum_error = 0.0
    for index, (frame, result) in enumerate(zip(request["calibration"], actual["calibration"], strict=True)):
        sm = SubMaster(liveCalibration=SimpleNamespace(rpyCalib=np.array(frame["rpy_bits"], dtype=np.uint32).view(np.float32),
                                                      calStatus=log.LiveCalibrationData.Status.calibrated if frame["calibrated"] else -1),
                       deviceState=SimpleNamespace(deviceType=frame["device"]), roadCameraState=SimpleNamespace(sensor=frame["sensor"]))
        sm.seen = {"roadCameraState": frame["road_seen"], "deviceState": frame["device_seen"]}
        sm.updated = {"liveCalibration": frame["updated"]}
        namespace.update(sm=sm, camera_yaw_trim_deg=frame["yaw_trim_degrees"], main_wide_camera=frame["main_wide"], use_extra_client=frame["use_extra"])
        updated = frame["updated"] and frame["road_seen"] and frame["device_seen"]
        try:
            exec(calibration, namespace)
        except KeyError:
            updated = None
        assert result["updated"] == updated and result["seen"] == namespace["live_calib_seen"]
        for name, key in (("model_transform_main", "main_bits"), ("model_transform_extra", "extra_bits")):
            expected = namespace[name].ravel()
            observed = np.array(result[key], dtype=np.uint32).view(np.float32)
            assert np.array_equal(np.isnan(expected), np.isnan(observed)), (index, name)
            assert np.allclose(expected, observed, rtol=1e-6, atol=1e-6, equal_nan=True), (index, name, expected, observed)
            finite = np.isfinite(expected) & np.isfinite(observed)
            maximum_error = max(maximum_error, float(np.max(np.abs(expected[finite] - observed[finite]), initial=0)))
            exact += int(np.sum((expected.view(np.uint32) == observed.view(np.uint32)) | (np.isnan(expected) & np.isnan(observed))))
            signed_zeros += int(np.sum((expected == 0) & (observed == 0) & (np.signbit(expected) != np.signbit(observed))))
            compared += expected.size
    return {"camera_pair_calls": sum(value["calls"] for value in request["pairs"]), "input_frames": len(request["frames"]),
                "calibration_entries": compared, "exact_bits_or_nan": exact, "maximum_absolute_error": maximum_error,
                "signed_zero_differences": signed_zeros, "calibration_tolerance": 1e-6, "device_validation": False}


def main():
    parser = argparse.ArgumentParser(description="Compare Rust model input state against original source")
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    request = fixtures()
    path = args.output / "request.json"
    path.write_text(json.dumps(request, allow_nan=False))
    result = args.output / "actual.json"
    subprocess.run([args.binary.resolve(), path.resolve(), result.resolve()], check=True)
    report = check(request, json.loads(result.read_text()))
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
