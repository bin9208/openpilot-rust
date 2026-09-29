from __future__ import annotations

import hashlib
import json
import os
import tempfile
from collections.abc import Mapping, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Literal

from tinygrad import Tensor, dtypes

from openpilot.system.camerad.cameras.nv12_info import get_nv12_info

from .compile import export_cpu
from .original import OriginalPipeline
from .qcom import export_qcom
from .schema import ExportError


@dataclass(frozen=True, slots=True)
class BundleSpec:
    kind: Literal["driving", "driver"]
    camera: tuple[int, int]
    original: OriginalPipeline
    sources: Mapping[str, str]


@dataclass(frozen=True, slots=True)
class TensorSpec:
    name: str
    shape: tuple[int, ...]
    dtype: str


@dataclass(frozen=True, slots=True)
class BundleEntry:
    kind: str
    camera: tuple[int, int]
    directory: str
    graph_sha256: str
    pipeline_sha256: str


def tensor_specs(tensors: Mapping[str, Tensor]) -> tuple[TensorSpec, ...]:
    specs = []
    names = {dtypes.uint8:"uint8", dtypes.float16:"float16", dtypes.float32:"float32"}
    for name, tensor in tensors.items():
        if tensor.dtype not in names or any(type(size) is not int or size <= 0 for size in tensor.shape):
            raise ExportError("unsupported pipeline tensor contract")
        specs.append(TensorSpec(name, tuple(tensor.shape), names[tensor.dtype]))
    return tuple(specs)


def emit(spec: BundleSpec, destination: Path, backend: Literal["cpu", "qcom"]) -> BundleEntry:
    original = spec.original
    exporter = export_cpu if backend == "cpu" else export_qcom
    exporter(original.pipeline.jit, original.bindings, destination, entrypoints=original.pipeline.entrypoints)
    stride, y_height, uv_height, frame_bytes = get_nv12_info(*spec.camera)
    descriptor = {"version":1, "kind":spec.kind, "camera":spec.camera,
                  "nv12":{"stride":stride, "y_height":y_height, "uv_height":uv_height, "bytes":frame_bytes},
                  "metadata":asdict(original.metadata), "sources":dict(spec.sources),
                  "inputs":[asdict(value) for value in tensor_specs(original.bindings.inputs)],
                  "outputs":[asdict(value) for value in tensor_specs(original.bindings.outputs)]}
    pipeline = (json.dumps(descriptor, sort_keys=True, indent=2) + "\n").encode()
    (destination / "pipeline.json").write_bytes(pipeline)
    return BundleEntry(spec.kind, spec.camera, destination.name,
                       hashlib.sha256((destination / "graph.json").read_bytes()).hexdigest(), hashlib.sha256(pipeline).hexdigest())


def publish(root: Path, specs: Sequence[BundleSpec], backend: Literal["cpu", "qcom"]) -> None:
    keys = [(spec.kind, spec.camera) for spec in specs]
    if not specs or len(specs) > 16 or len(set(keys)) != len(keys):
        raise ExportError("empty, excessive or duplicate pipeline bundle selection")
    if backend not in {"cpu", "qcom"}:
        raise ExportError("unsupported native pipeline backend")
    for spec in specs:
        if spec.kind not in {"driving", "driver"} or spec.camera not in {(1928,1208), (1344,760)}:
            raise ExportError("unsupported pipeline kind or camera")
        if not spec.sources or any(not role or len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest)
                                   for role, digest in spec.sources.items()):
            raise ExportError("source SHA-256 provenance required")
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".build-", dir=root) as temporary:
        stage = Path(temporary)
        generation = stage / "generation"
        generation.mkdir()
        entries = [emit(spec, generation / f"{spec.kind}-{spec.camera[0]}x{spec.camera[1]}", backend) for spec in specs]
        index = (json.dumps({"version":1, "bundles":[asdict(entry) for entry in entries]}, sort_keys=True, indent=2) + "\n").encode()
        (generation / "index.json").write_bytes(index)
        digest = hashlib.sha256(index).hexdigest()
        destination = root / digest
        if destination.exists():
            for path in generation.rglob("*"):
                if path.is_file():
                    existing = destination / path.relative_to(generation)
                    if not existing.is_file() or hashlib.sha256(existing.read_bytes()).digest() != hashlib.sha256(path.read_bytes()).digest():
                        raise ExportError("existing generation contents disagree with immutable artifact")
        else:
            generation.rename(destination)
        pointer = stage / "current.json"
        with pointer.open("wb") as output:
            output.write((json.dumps({"version":1, "generation":digest}, sort_keys=True) + "\n").encode())
            output.flush()
            os.fsync(output.fileno())
        os.replace(pointer, root / "current.json")
