# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with the original-model environment: PYTHONPATH=.:tinygrad_repo:rust/tools
# DEV=CPU:LLVM JIT=1 JIT_BATCH_SIZE=0 uv run rust/tools/check_input_pipeline.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace

import numpy as np

from check_model_inputs import SubMaster, source_nodes
from check_pipeline_reference import execute, write_tensor
from model_export import export_cpu
from model_export.original import driving
from openpilot.cereal import log
from openpilot.common.filter_simple import FirstOrderFilter
from openpilot.common.transformations.camera import DEVICE_CAMERAS
from openpilot.common.transformations.model import get_warp_matrix
from openpilot.selfdrive.modeld.compile_modeld import get_policy_npy_shapes


@dataclass(frozen=True, slots=True)
class Comparison:
    inputs_binary: Path
    model_binary: Path
    trusted_driving: Path
    output: Path


def compare(settings: Comparison, resolution: tuple[int, int]) -> bool:
    """Compare source-derived inputs and original model state with Rust execution."""
    destination = settings.output / f"{resolution[0]}x{resolution[1]}"
    destination.mkdir()
    (destination / "original-model").mkdir()
    original = driving(settings.trusted_driving, resolution)
    export_cpu(original.pipeline.jit, original.bindings, destination / "bundle", entrypoints=original.pipeline.entrypoints)
    drops, calibration, pulse, _ = source_nodes()
    drop_scope = {"frame_dropped_filter": FirstOrderFilter(0., 10., .05), "last_vipc_frame_id": 0, "run_count": 0}
    calib_scope = {"np": np, "log": log, "DEVICE_CAMERAS": DEVICE_CAMERAS, "get_warp_matrix": get_warp_matrix,
                   "model_transform_main": np.zeros((3, 3), dtype=np.float32),
                   "model_transform_extra": np.zeros((3, 3), dtype=np.float32), "live_calib_seen": False}
    shapes, sizes = get_policy_npy_shapes(original.metadata.input_shapes)
    packed = np.zeros(sum(sizes), dtype=np.float32)
    fields = {key: value.reshape(shape) for (key, shape), value in
              zip(shapes.items(), np.split(packed, np.cumsum(sizes[:-1])), strict=True)}
    model_inputs = SimpleNamespace(prev_desire=np.zeros(8, dtype=np.float32), npy=fields)
    hidden = original.metadata.output_slices["hidden_state"]
    request = {"feature_count": hidden[1] - hidden[0], "pairs": [], "streams": [], "frames": [], "calibration": []}
    source_results = []
    references = {}
    sequence = []
    random = np.random.default_rng(20260930)
    desires = [0, 1, 1, 0, 1, 2, 2, 0, 2, 3, 3, 0]
    for index, desire in enumerate(desires):
        frame_id = index + 1 + int(index >= 5)
        frame = {"frame_id": frame_id, "desire": desire, "is_rhd": index >= 6, "lateral_time": index / 20,
                 "longitudinal_time": .2 + index / 40, "features": None, "reset": False}
        angles = np.array([.001 * (index + 1), -.015, .007], dtype=np.float32)
        calib = {"updated": index % 4 == 0, "road_seen": True, "device_seen": True,
                 "rpy_bits": angles.view(np.uint32).tolist(), "calibrated": index >= 4, "yaw_trim_degrees": .35,
                 "device": "tici", "sensor": "os04c10" if resolution[0] == 1344 else "ar0231",
                 "main_wide": False, "use_extra": True}
        sm = SubMaster(liveCalibration=SimpleNamespace(rpyCalib=angles, calStatus=log.LiveCalibrationData.Status.calibrated if calib["calibrated"] else -1),
                       deviceState=SimpleNamespace(deviceType=calib["device"]), roadCameraState=SimpleNamespace(sensor=calib["sensor"]))
        sm.seen = {"roadCameraState": calib["road_seen"], "deviceState": calib["device_seen"]}
        sm.updated = {"liveCalibration": calib["updated"]}
        calib_scope.update(sm=sm, camera_yaw_trim_deg=calib["yaw_trim_degrees"], main_wide_camera=calib["main_wide"], use_extra_client=calib["use_extra"])
        exec(calibration, calib_scope)
        drop_scope["meta_main"] = SimpleNamespace(frame_id=frame_id)
        exec(drops, drop_scope)
        drop_scope["last_vipc_frame_id"] = frame_id
        prepare_only = drop_scope["prepare_only"]
        desire_vector = np.zeros(8, dtype=np.float32)
        desire_vector[desire] = 1
        values = {"desire_pulse": desire_vector, "traffic_convention": np.array([not frame["is_rhd"], frame["is_rhd"]]),
                  "action_t": np.array([frame["lateral_time"], frame["longitudinal_time"]], dtype=np.float32)}
        exec(pulse, {"np": np, "self": model_inputs, "inputs": values})
        expected = {"drop": {"dropped": drop_scope["vipc_dropped_frames"], "ratio": drop_scope["frame_drop_ratio"], "prepare_only": prepare_only},
                    "packed_bits": packed.view(np.uint32).tolist(), "calibration": {}}
        inputs = {}
        for name in ("frame", "big_frame"):
            image = random.integers(0, 256, original.bindings.inputs[name].shape, dtype=np.uint8)
            write_tensor(original.bindings.inputs[name], image)
            inputs[name] = f"{name}-{index}.bin"
            (destination / inputs[name]).write_bytes(image.tobytes())
        for name, source in (("tfm", "model_transform_main"), ("big_tfm", "model_transform_extra")):
            matrix = calib_scope[source]
            write_tensor(original.bindings.inputs[name], matrix)
            expected["calibration"][name] = matrix.ravel().view(np.uint32).tolist()
            inputs[name] = f"{name}-{index}.bin"
        write_tensor(original.bindings.inputs["packed_npy_inputs"], packed)
        inputs["packed_npy_inputs"] = f"packed-{index}.bin"
        execute(original.stages[0])
        stages = ["prepare"]
        if not prepare_only:
            execute(original.stages[1])
            stages.append("policy")
            model = original.bindings.outputs["model"].numpy().reshape(-1)
            if not np.all(np.isfinite(model)):
                raise RuntimeError(f"non-finite original model output: {resolution}, frame {index}")
            fields["prev_feat"][:] = model[hidden[0]:hidden[1]]
            frame["features"] = model[hidden[0]:hidden[1]].tolist()
        outputs = {}
        for name, tensor in original.bindings.outputs.items():
            filename = f"{name}-{index}.bin"
            output_bytes = tensor.numpy().tobytes()
            references[filename] = hashlib.sha256(output_bytes).hexdigest()
            if name == "model":
                (destination / "original-model" / filename).write_bytes(output_bytes)
            outputs[name] = filename
        for stage_index, stage in enumerate(stages):
            sequence.append({"entrypoint": stage, "inputs": inputs if stage_index == 0 else {},
                             "outputs": outputs if stage_index == len(stages) - 1 else {}})
        source_results.append(expected)
        request["frames"].append(frame)
        request["calibration"].append(calib)
        print(f"original {resolution}: frame {index + 1}/{len(desires)}, prepare_only={prepare_only}", flush=True)
    (destination / "request.json").write_text(json.dumps(request, allow_nan=False))
    (destination / "source-inputs.json").write_text(json.dumps(source_results, allow_nan=False))
    (destination / "original-hashes.json").write_text(json.dumps(references, indent=2) + "\n")
    subprocess.run([settings.inputs_binary, destination / "request.json", destination / "rust-inputs.json"], check=True)
    actual = json.loads((destination / "rust-inputs.json").read_text())
    calibration_exact = calibration_count = 0
    for index, (expected, observed, calib) in enumerate(zip(source_results, actual["frames"], actual["calibration"], strict=True)):
        if expected["drop"] != observed["drop"] or expected["packed_bits"] != observed["packed_bits"]:
            raise RuntimeError(f"source/Rust policy inputs differ: {resolution}, frame {index}")
        if calib["updated"] != request["calibration"][index]["updated"] or not calib["seen"]:
            raise RuntimeError(f"source/Rust calibration state differs: {resolution}, frame {index}")
        for name, key in (("tfm", "main_bits"), ("big_tfm", "extra_bits")):
            bits = np.array(calib[key], dtype=np.uint32)
            calibration_exact += int(np.sum(bits == np.array(expected["calibration"][name], dtype=np.uint32)))
            calibration_count += bits.size
            (destination / f"{name}-{index}.bin").write_bytes(bits.tobytes())
        (destination / f"packed-{index}.bin").write_bytes(np.array(observed["packed_bits"], dtype=np.uint32).tobytes())
    (destination / "sequence.json").write_text(json.dumps(sequence))
    subprocess.run([settings.model_binary, "--trusted-bundle", destination / "bundle", destination / "sequence.json"], check=True)
    native_hashes = {name: hashlib.sha256((destination / name).read_bytes()).hexdigest() for name in references}
    (destination / "native-hashes.json").write_text(json.dumps(native_hashes, indent=2) + "\n")
    mismatches = [name for name, expected in references.items() if native_hashes[name] != expected]
    differences = []
    for name in mismatches:
        if name.startswith("model-"):
            expected = np.fromfile(destination / "original-model" / name, dtype=np.float32)
            observed = np.fromfile(destination / name, dtype=np.float32)
            indices = np.flatnonzero(expected.view(np.uint32) != observed.view(np.uint32))
            differences.append({"file": name, "different_elements": int(indices.size),
                                "maximum_absolute_error": float(np.max(np.abs(expected - observed))),
                                "samples": [{"index": int(i), "original": float(expected[i]), "native": float(observed[i])} for i in indices[:8]]})
    report = {"resolution": resolution, "frames": len(desires), "prepare_only_frames": sum(item["drop"]["prepare_only"] for item in source_results),
              "source_inputs_equal": True, "calibration_exact_entries": calibration_exact, "calibration_entries": calibration_count,
              "output_files_compared": len(references), "outputs": list(original.bindings.outputs), "mismatches": mismatches,
              "comparison": "identical output bytes" if not mismatches else "output bytes differ", "device_validation": False,
              "model_checkpoint": original.metadata.model_checkpoint, "numeric_differences": differences,
              "llvm_path": os.environ.get("LLVM_PATH")}
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report), flush=True)
    return not mismatches


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare source input state and original driving pipeline with Rust helpers and native execution")
    parser.add_argument("--inputs-binary", type=Path, required=True)
    parser.add_argument("--model-binary", type=Path, required=True)
    parser.add_argument("--trusted-driving", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    settings = Comparison(args.inputs_binary.resolve(), args.model_binary.resolve(), args.trusted_driving.resolve(), args.output.resolve())
    settings.output.mkdir(parents=True, exist_ok=False)
    results = [compare(settings, resolution) for resolution in ((1344, 760), (1928, 1208))]
    (settings.output / "report.json").write_text(json.dumps({"resolutions": [[1344, 760], [1928, 1208]], "frames_per_resolution": 12,
        "comparison": "identical output bytes" if all(results) else "output bytes differ", "device_validation": False,
        "llvm_path": os.environ.get("LLVM_PATH")}, indent=2) + "\n")
    if not all(results):
        raise RuntimeError(f"original/native output bytes differ; see {settings.output}/report.json")


if __name__ == "__main__":
    main()
