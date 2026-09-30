# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "zstandard==0.25.0"]
# ///
# Run with PYTHONPATH=.:tinygrad_repo DEV=CPU:LLVM CPU_COUNT=4 JIT=2 uv run rust/tools/check_policy_reference.py --help
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import numpy as np
from tinygrad import Tensor, TinyJit
from tinygrad.nn.onnx import OnnxRunner

from model_export import Bindings, export_cpu
from openpilot.selfdrive.modeld.compile_modeld import POLICY_INPUTS, make_input_queues, make_run_policy
from openpilot.selfdrive.modeld.get_model_metadata import make_metadata_dict


def compare_policy(model: Path, destination: Path, binary: Path) -> None:
    destination.mkdir(parents=True, exist_ok=False)
    metadata = make_metadata_dict(model)
    queues, numpy_inputs = make_input_queues(metadata["input_shapes"], 4, "CPU")
    random = np.random.default_rng(20260930)
    warped = Tensor(random.integers(0, 256, (2, 6, 128, 256), dtype=np.uint8)).realize()
    inputs = {key: queues[key] for key in POLICY_INPUTS} | {"warped": warped}
    jit = TinyJit(make_run_policy(OnnxRunner(model), metadata, 4), prune=True)
    print("policy: capture original queues and model", flush=True)
    jit(**inputs)
    result = jit(**inputs)[0]
    for name in ("img_q", "big_img_q", "feat_q", "desire_q"):
        queues[name].assign(Tensor.zeros(queues[name].shape, dtype=queues[name].dtype)).realize()
    outputs = {"model": result} | {name: queues[name] for name in ("img_q", "big_img_q", "feat_q", "desire_q")}
    export_cpu(jit, Bindings(inputs, outputs), destination / "bundle")
    frames = []
    references = []
    previous = np.zeros((1, 512), dtype=np.float32)
    print("policy: 128 frames with recurrent feedback and history turnover", flush=True)
    for frame in range(128):
        image = random.integers(0, 256, warped.shape, dtype=np.uint8)
        warped.assign(Tensor(image)).realize()
        numpy_inputs["desire"][:] = 0
        numpy_inputs["desire"][frame % 8] = 1
        numpy_inputs["traffic_convention"][:] = [1, 0]
        numpy_inputs["action_t"][:] = [0.0, (frame % 20) / 20.0]
        numpy_inputs["prev_feat"][:] = previous
        frame_inputs = {}
        for name in ("warped", "packed_npy_inputs"):
            filename = f"{name}-{frame}.bin"
            (destination / filename).write_bytes(inputs[name].numpy().tobytes())
            frame_inputs[name] = filename
        model_output = jit(**inputs)[0].numpy().copy()
        if not np.all(np.isfinite(model_output)):
            raise RuntimeError(f"non-finite model output in frame {frame}")
        previous = model_output[:, metadata["output_slices"]["hidden_state"]]
        references.append({name: tensor.numpy().copy() for name, tensor in outputs.items()})
        frames.append({"inputs": frame_inputs, "outputs": {name: f"{name}-{frame}.bin" for name in outputs}})
    sequence = destination / "sequence.json"
    sequence.write_text(json.dumps(frames, indent=2) + "\n")
    subprocess.run([binary, "--trusted-bundle", destination / "bundle", sequence], check=True)
    elements = 0
    for frame, expected in enumerate(references):
        for name, reference in expected.items():
            actual = np.fromfile(destination / f"{name}-{frame}.bin", dtype=reference.dtype).reshape(reference.shape)
            np.testing.assert_array_equal(actual, reference)
            elements += actual.size
    graph = json.loads((destination / "bundle" / "graph.json").read_text())
    report = {
        "model_sha256": hashlib.sha256(model.read_bytes()).hexdigest(), "backend": graph["backend"],
        "frames": 128, "frame_skip": 4, "compared_elements": elements, "outputs": list(outputs),
        "tolerance": "exact equality", "maximum_absolute_error": 0, "call_count": len(graph["calls"]),
        "device_validation": False,
    }
    (destination / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2), flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare the original recurrent driving policy and queues with the Rust executor")
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    compare_policy(args.model.resolve(), args.output.resolve(), args.binary.resolve())


if __name__ == "__main__":
    main()
