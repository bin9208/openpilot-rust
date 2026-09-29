# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy>=2.0", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo DEV=CPU:LLVM JIT_BATCH_SIZE=0 uv run rust/tools/check_pipeline_reference.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from numpy.typing import NDArray
from tinygrad import Tensor
from tinygrad.engine.jit import _prepare_jit_inputs

from model_export import export_cpu
from model_export.binding_order import ordered_inputs
from model_export.original import DriverArtifacts, OriginalPipeline, driver, driving
from model_export.pipeline import Stage
from openpilot.selfdrive.modeld.compile_modeld import get_policy_npy_shapes


def execute(stage: Stage) -> None:
    inputs, variables, _, _ = _prepare_jit_inputs(ordered_inputs(stage.jit.captured.expected_names, stage.inputs), {})
    stage.jit.captured(inputs, variables)


def write_tensor(tensor: Tensor, values: NDArray) -> None:
    buffer = tensor.uop.base.buffer
    if buffer.device == "NPY":
        buffer.as_memoryview(force_zero_copy=True)[:] = memoryview(values).cast("B")
    else:
        buffer.copyin(memoryview(values))


@dataclass(frozen=True, slots=True)
class Comparison:
    output: Path
    binary: Path
    frames: int


def compare(original: OriginalPipeline, settings: Comparison) -> None:
    destination = settings.output
    destination.mkdir(parents=True, exist_ok=False)
    export_cpu(original.pipeline.jit, original.bindings, destination / "bundle", entrypoints=original.pipeline.entrypoints)
    print("original pipeline exported", original.pipeline.entrypoints, flush=True)
    random = np.random.default_rng(20260930)
    road = original.stages[1].name == "policy"
    hidden = original.metadata.output_slices.get("hidden_state", (0, 0))
    previous = np.zeros(hidden[1] - hidden[0], dtype=np.float32)
    references = {}
    sequence = []
    compared = 0
    skipped = 0
    for frame in range(settings.frames):
        inputs = {}
        for name in (("frame", "big_frame") if road else ("frame",)):
            values = random.integers(0, 256, original.bindings.inputs[name].shape, dtype=np.uint8)
            write_tensor(original.bindings.inputs[name], values)
            filename = f"{name}-{frame}.bin"
            (destination / filename).write_bytes(values.tobytes())
            inputs[name] = filename
        matrix = np.array([[0.5, 0.01, -40 + frame % 3], [-0.01, 0.8, -30], [0.0002, -0.0001, 1]], dtype=np.float32)
        for name in (("tfm", "big_tfm") if road else ("transform",)):
            write_tensor(original.bindings.inputs[name], matrix)
            filename = f"{name}-{frame}.bin"
            (destination / filename).write_bytes(matrix.tobytes())
            inputs[name] = filename
        if road:
            shapes, sizes = get_policy_npy_shapes(original.metadata.input_shapes)
            packed = np.zeros(sum(sizes), dtype=np.float32)
            fields = {key:value.reshape(shape) for (key, shape), value in zip(shapes.items(), np.split(packed, np.cumsum(sizes[:-1])), strict=True)}
            fields["desire"][frame % fields["desire"].size] = 1
            fields["traffic_convention"][:] = [1, 0]
            fields["action_t"][:] = [0, (frame % 20) / 20]
            fields["prev_feat"][:] = previous
            name, values = "packed_npy_inputs", packed
        else:
            name, values = "calib", np.array([[0.01 * frame, -0.02, 0.03]], dtype=np.float32)
        write_tensor(original.bindings.inputs[name], values)
        filename = f"{name}-{frame}.bin"
        (destination / filename).write_bytes(values.tobytes())
        inputs[name] = filename
        execute(original.stages[0])
        stages = ["prepare"]
        prepare_only = road and frame % 11 == 5
        if prepare_only:
            skipped += 1
        else:
            execute(original.stages[1])
            stages.append(original.stages[1].name)
            model = original.bindings.outputs["model"].numpy().reshape(-1)
            if not np.all(np.isfinite(model)):
                raise RuntimeError(f"non-finite original output in frame {frame}")
            if road:
                previous = model[hidden[0]:hidden[1]].copy()
        outputs = {}
        for name, tensor in original.bindings.outputs.items():
            values = tensor.numpy()
            filename = f"{name}-{frame}.bin"
            references[filename] = hashlib.sha256(values.tobytes()).hexdigest()
            outputs[name] = filename
            compared += values.size
        for index, stage in enumerate(stages):
            sequence.append({"entrypoint":stage, "inputs":inputs if index == 0 else {},
                             "outputs":outputs if index == len(stages) - 1 else {}})
        if frame % 16 == 0:
            print(f"original pipeline frame {frame}/{settings.frames}", flush=True)
    (destination / "sequence.json").write_text(json.dumps(sequence))
    subprocess.run([settings.binary, "--trusted-bundle", destination / "bundle", destination / "sequence.json"], check=True)
    for filename, expected in references.items():
        if hashlib.sha256((destination / filename).read_bytes()).hexdigest() != expected:
            raise RuntimeError(f"original/native output bytes differ: {filename}")
    report = {"frames":settings.frames, "prepare_only_frames":skipped, "compared_elements":compared,
              "comparison":"identical output bytes", "outputs":list(original.bindings.outputs),
              "device_validation":False, "model_checkpoint":original.metadata.model_checkpoint}
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report), flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare trusted original compiled pipelines with the native Rust process")
    parser.add_argument("--trusted-driving", type=Path)
    parser.add_argument("--trusted-driver", type=Path)
    parser.add_argument("--trusted-warp", type=Path)
    parser.add_argument("--trusted-metadata", type=Path)
    parser.add_argument("--resolution", type=int, nargs=2, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=128)
    args = parser.parse_args()
    if args.frames < 1:
        parser.error("frames must be positive")
    if args.trusted_driving is not None:
        original = driving(args.trusted_driving, tuple(args.resolution))
    elif args.trusted_driver is not None and args.trusted_warp is not None and args.trusted_metadata is not None:
        original = driver(DriverArtifacts(args.trusted_driver, args.trusted_warp, args.trusted_metadata))
    else:
        parser.error("provide a trusted driving artifact or all three trusted driver artifacts")
    compare(original, Comparison(args.output.resolve(), args.binary.resolve(), args.frames))


if __name__ == "__main__":
    main()
