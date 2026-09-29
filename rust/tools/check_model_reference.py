# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy>=2.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo DEV=CPU:LLVM CPU_COUNT=4 JIT=2 uv run rust/tools/check_model_reference.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import numpy as np
from tinygrad import Tensor, TinyJit, dtypes
from tinygrad.dtype import DType
from tinygrad.nn.onnx import OnnxRunner

from model_export import Bindings, export_cpu


def random_input(spec: tuple[tuple[int, ...], DType], random: np.random.Generator) -> np.ndarray:
    shape, dtype = spec
    if dtype == dtypes.uint8:
        return random.integers(0, 256, shape, dtype=np.uint8)
    numpy_dtype = {dtypes.float16: np.float16, dtypes.float32: np.float32}[dtype]
    return random.standard_normal(shape).astype(numpy_dtype)


def compare_model(model: Path, destination: Path, binary: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    print(f"loading {model}", flush=True)
    runner = OnnxRunner(model)
    names = list(runner.graph_inputs)
    random = np.random.default_rng(20260930)
    inputs = {name: Tensor(random_input((tuple(spec.shape), spec.dtype), random)).realize()
              for name, spec in runner.graph_inputs.items()}

    @TinyJit
    def forward(*values: Tensor) -> tuple[Tensor, ...]:
        return tuple(value.cast("float32").realize() for value in runner(dict(zip(names, values, strict=True))).values())

    print("capture: first execution", flush=True)
    forward(*inputs.values())
    print("capture: second execution", flush=True)
    outputs = forward(*inputs.values())
    output_bindings = {f"output{index}": value for index, value in enumerate(outputs)}
    print("export: compiled kernels and buffer graph", flush=True)
    export_cpu(forward, Bindings(inputs, output_bindings), destination / "bundle")
    frames = []
    expected: list[list[np.ndarray]] = []
    for frame in range(3):
        frame_inputs = {}
        for index, (name, tensor) in enumerate(inputs.items()):
            values = random_input((tuple(tensor.shape), tensor.dtype), random)
            tensor.assign(Tensor(values)).realize()
            filename = f"input-{frame}-{index}.bin"
            (destination / filename).write_bytes(values.tobytes())
            frame_inputs[name] = filename
        expected.append([tensor.numpy().copy() for tensor in forward(*inputs.values())])
        frames.append({"inputs": frame_inputs, "outputs": {name: f"{name}-{frame}.bin" for name in output_bindings}})
    sequence = destination / "sequence.json"
    sequence.write_text(json.dumps(frames, indent=2) + "\n")
    print("compare: Rust process (three frames)", flush=True)
    subprocess.run([binary, "--trusted-bundle", destination / "bundle", sequence], check=True)
    maximum = 0.0
    elements = 0
    for frame, references in enumerate(expected):
        for index, reference in enumerate(references):
            actual = np.fromfile(destination / f"output{index}-{frame}.bin", dtype=np.float32).reshape(reference.shape)
            if not np.all(np.isfinite(reference)) or not np.all(np.isfinite(actual)):
                raise RuntimeError(f"non-finite model output in frame {frame}, output {index}")
            np.testing.assert_array_equal(actual, reference)
            maximum = max(maximum, float(np.max(np.abs(actual - reference))))
            elements += actual.size
    graph = json.loads((destination / "bundle" / "graph.json").read_text())
    report = {
        "model_sha256": hashlib.sha256(model.read_bytes()).hexdigest(),
        "backend": graph["backend"], "frames": 3, "compared_elements": elements,
        "tolerance": "exact float32 equality", "maximum_absolute_error": maximum,
        "kernel_count": len(graph["kernels"]), "call_count": len(graph["calls"]),
        "allocation_bytes": sum(allocation["bytes"] for allocation in graph["allocations"]),
        "input_dtypes": {name: str(tensor.dtype) for name, tensor in inputs.items()},
        "device_validation": False,
    }
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2), flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare an actual ONNX model's tinygrad CPU execution with the native Rust executor")
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    compare_model(args.model.resolve(), args.output.resolve(), args.binary.resolve())


if __name__ == "__main__":
    main()
