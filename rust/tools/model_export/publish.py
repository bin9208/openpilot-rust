from __future__ import annotations

import hashlib
import json
import os
import selectors
import stat
import subprocess
import tempfile
import time
from collections.abc import Mapping, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Literal

from tinygrad import Tensor, dtypes

from openpilot.system.camerad.cameras.nv12_info import get_nv12_info

from .compile import export_cpu
from .native import _admitted_helper, _identity, _receipt_object
from .original import OriginalPipeline
from .qcom import export_qcom
from .schema import ExportError


@dataclass(frozen=True, slots=True)
class BundleSpec:
    kind: Literal["driving", "driver"]
    camera: tuple[int, int]
    original: OriginalPipeline
    sources: Mapping[str, str]
    cached: object | None = None


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
    if spec.cached is not None:
        if backend != "cpu":
            raise ExportError("cached native mode requires the CPU backend")
        export_cpu(original.pipeline.jit, original.bindings, destination, entrypoints=original.pipeline.entrypoints, cached=spec.cached)
    else:
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


def _generation_roster(directory: Path) -> dict[Path, bool]:
    if directory.is_symlink() or not directory.is_dir():
        raise ExportError("existing generation is not a regular artifact directory")
    roster = {}
    for path in directory.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ExportError("existing generation contains an alias or special file")
        roster[path.relative_to(directory)] = path.is_dir()
    return roster


def _publication_helper():
    return _admitted_helper(os.environ.get("MODEL_CATALOG_PUBLISH_BINARY"), os.environ.get("MODEL_CATALOG_PUBLISH_SHA256"))


def catalog_publisher_binding() -> dict:
    # Reuse native.py's existing held-FD/native-ELF/SHA/identity admission.
    # This optional issuer does not change model compilation or target selection.
    try:
        with _publication_helper() as helper:
            return {"enabled":False} if helper is None else helper[1]
    except ExportError as error:
        raise ExportError("catalog publisher admission failed: " + str(error)) from error


def _publication_file(path: Path, limit: int) -> bytes:
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or not 0 <= before.st_size <= limit:
            raise ExportError("catalog publisher output is not a bounded regular file")
        data = os.pread(fd, before.st_size + 1, 0)
        if (len(data) != before.st_size or _identity(before) != _identity(os.fstat(fd))
                or _identity(before) != _identity(path.lstat())):
            raise ExportError("catalog publisher output changed while reading")
        return data
    finally:
        os.close(fd)


def _run_publisher(fd: int, arguments: list[str]) -> bytes:
    # The maintained preparer owns the compiler process group. This direct
    # native helper has no subprocesses; drain both streams within live bounds.
    child = subprocess.Popen([f"/proc/self/fd/{fd}", *arguments], pass_fds=(fd,),
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
    output, errors = bytearray(), bytearray()
    deadline = time.monotonic() + 120
    try:
        with selectors.DefaultSelector() as selector:
            for stream in (child.stdout, child.stderr):
                os.set_blocking(stream.fileno(), False)
                selector.register(stream, selectors.EVENT_READ)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ExportError("catalog publisher deadline exceeded")
                for key, _ in selector.select(min(remaining, 0.1)):
                    try:
                        chunk = os.read(key.fileobj.fileno(), 65536)
                    except BlockingIOError:
                        continue
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    target = output if key.fileobj is child.stdout else errors
                    if len(target) + len(chunk) > 65536:
                        raise ExportError("catalog publisher diagnostic output exceeds bound")
                    target.extend(chunk)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ExportError("catalog publisher deadline exceeded")
        if child.wait(timeout=remaining) != 0:
            raise ExportError("native catalog publisher failed: " + errors.decode("utf-8", errors="replace"))
        return bytes(output)
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
        child.stdout.close()
        child.stderr.close()


def _native_publish(root: Path, generation: Path, entries: Sequence[BundleEntry], binding: dict) -> None:
    request = generation.parent / "entries.json"
    data = json.dumps([asdict(entry) for entry in entries], sort_keys=True).encode()
    if len(data) > 1024 * 1024:
        raise ExportError("native catalog entry request exceeds bound")
    with request.open("xb") as output:
        output.write(data)
    try:
        with _publication_helper() as helper:
            if helper is None or helper[1] != binding:
                raise ExportError("catalog publisher binding changed during emission")
            fd = helper[0]
            output = _run_publisher(fd, ["--root", str(root.resolve(strict=True)),
                                         "--staged-generation", str(generation.resolve(strict=True)),
                                         "--entries", str(request.resolve(strict=True))])
            receipt = json.loads(output, object_pairs_hook=_receipt_object)
            if (not isinstance(receipt, dict) or set(receipt) != {"generation", "reused"}
                    or type(receipt["reused"]) is not bool or not isinstance(receipt["generation"], str)
                    or len(receipt["generation"]) != 64 or any(c not in "0123456789abcdef" for c in receipt["generation"])):
                raise ExportError("invalid native catalog publication receipt")
            pointer_bytes = _publication_file(root / "current.json", 4096)
            pointer = json.loads(pointer_bytes, object_pairs_hook=_receipt_object)
            if (not isinstance(pointer, dict) or set(pointer) != {"generation", "version"}
                    or type(pointer["version"]) is not int or pointer["version"] != 1
                    or pointer["generation"] != receipt["generation"]
                    or pointer_bytes != (json.dumps({"version":1, "generation":receipt["generation"]}, sort_keys=True) + "\n").encode()):
                raise ExportError("native catalog pointer differs from publication receipt")
            index = _publication_file(root / receipt["generation"] / "index.json", 1024 * 1024)
            expected_index = (json.dumps({"version":1, "bundles":json.loads(data)}, sort_keys=True, indent=2) + "\n").encode()
            if hashlib.sha256(index).hexdigest() != receipt["generation"] or index != expected_index:
                raise ExportError("native catalog index differs from actual emitted entries")
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        raise ExportError("native catalog publication failed: " + str(error)) from error


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
    publisher_before = catalog_publisher_binding()
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".build-", dir=root) as temporary:
        stage = Path(temporary)
        generation = stage / "generation"
        generation.mkdir()
        entries = [emit(spec, generation / f"{spec.kind}-{spec.camera[0]}x{spec.camera[1]}", backend) for spec in specs]
        if catalog_publisher_binding() != publisher_before:
            raise ExportError("catalog publisher binding changed during emission")
        if publisher_before["enabled"]:
            _native_publish(root, generation, entries, publisher_before)
            return
        index = (json.dumps({"version":1, "bundles":[asdict(entry) for entry in entries]}, sort_keys=True, indent=2) + "\n").encode()
        (generation / "index.json").write_bytes(index)
        digest = hashlib.sha256(index).hexdigest()
        destination = root / digest
        if destination.exists() or destination.is_symlink():
            roster = _generation_roster(generation)
            if roster != _generation_roster(destination):
                raise ExportError("existing generation file roster disagrees with immutable artifact")
            for relative, is_directory in roster.items():
                if not is_directory:
                    existing, path = destination / relative, generation / relative
                    if hashlib.sha256(existing.read_bytes()).digest() != hashlib.sha256(path.read_bytes()).digest():
                        raise ExportError("existing generation contents disagree with immutable artifact")
        else:
            generation.rename(destination)
        pointer = stage / "current.json"
        with pointer.open("wb") as output:
            output.write((json.dumps({"version":1, "generation":digest}, sort_keys=True) + "\n").encode())
            output.flush()
            os.fsync(output.fileno())
        os.replace(pointer, root / "current.json")
