from __future__ import annotations

import hashlib
import json
import os
import tempfile
from collections.abc import Sequence
from dataclasses import asdict
from pathlib import Path

from openpilot.system.camerad.cameras.nv12_info import get_nv12_info

from .offline_compiler import OfflineQcomCompiler
from .offline_pipeline import OfflinePipelineSpec, build_pipeline, export_pipeline
from .publish import BundleEntry, tensor_specs
from .schema import ExportError


def emit(spec: OfflinePipelineSpec, compiler: OfflineQcomCompiler, destination: Path) -> BundleEntry:
    pipeline = build_pipeline(spec)
    export_pipeline(pipeline, compiler, destination)
    stride, y_height, uv_height, frame_bytes = get_nv12_info(*spec.camera)
    descriptor = {
        "version": 1, "kind": spec.kind, "camera": spec.camera,
        "nv12": {"stride": stride, "y_height": y_height, "uv_height": uv_height, "bytes": frame_bytes},
        "metadata": asdict(pipeline.metadata), "sources": {"onnx": spec.sha256},
        "inputs": [asdict(value) for value in tensor_specs(pipeline.model.bindings.inputs)],
        "outputs": [asdict(value) for value in tensor_specs(pipeline.model.bindings.outputs)],
    }
    descriptor_bytes = (json.dumps(descriptor, sort_keys=True, indent=2) + "\n").encode()
    (destination / "pipeline.json").write_bytes(descriptor_bytes)
    return BundleEntry(spec.kind, spec.camera, destination.name,
                       hashlib.sha256((destination / "graph.json").read_bytes()).hexdigest(),
                       hashlib.sha256(descriptor_bytes).hexdigest())


def publish(root: Path, specs: Sequence[OfflinePipelineSpec], compiler: OfflineQcomCompiler) -> None:
    keys = [(spec.kind, spec.camera) for spec in specs]
    if not specs or len(specs) > 4 or len(set(keys)) != len(keys):
        raise ExportError("empty, excessive or duplicate offline pipeline selection")
    for spec in specs:
        if spec.kind not in {"driving", "driver"} or spec.camera not in {(1928, 1208), (1344, 760)}:
            raise ExportError("unsupported offline pipeline kind or camera")
        if len(spec.sha256) != 64 or any(char not in "0123456789abcdef" for char in spec.sha256):
            raise ExportError("offline pipeline source SHA-256 required")
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".build-", dir=root) as temporary:
        stage = Path(temporary)
        generation = stage / "generation"
        generation.mkdir()
        entries = [emit(spec, compiler, generation / f"{spec.kind}-{spec.camera[0]}x{spec.camera[1]}") for spec in specs]
        index = (json.dumps({"version": 1, "bundles": [asdict(entry) for entry in entries]}, sort_keys=True, indent=2) + "\n").encode()
        (generation / "index.json").write_bytes(index)
        digest = hashlib.sha256(index).hexdigest()
        destination = root / digest
        if destination.exists():
            expected = {path.relative_to(generation) for path in generation.rglob("*") if path.is_file()}
            actual = {path.relative_to(destination) for path in destination.rglob("*") if path.is_file()}
            if destination.is_symlink() or expected != actual:
                raise ExportError("existing generation disagrees with immutable artifact")
            for name in expected:
                if hashlib.sha256((destination / name).read_bytes()).digest() != hashlib.sha256((generation / name).read_bytes()).digest():
                    raise ExportError("existing generation contents disagree with immutable artifact")
        else:
            generation.rename(destination)
        pointer = stage / "current.json"
        with pointer.open("wb") as output:
            output.write((json.dumps({"version": 1, "generation": digest}, sort_keys=True) + "\n").encode())
            output.flush()
            os.fsync(output.fileno())
        os.replace(pointer, root / "current.json")
