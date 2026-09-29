from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Mapping

from tinygrad import Tensor


class ExportError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class Entrypoint:
    name: str
    start: int
    end: int


def entrypoint_version(entries: tuple[Entrypoint, ...], calls: int) -> int:
    if not entries:
        return 1
    names: set[str] = set()
    end = 0
    for entry in entries:
        if not entry.name or entry.name in names or entry.start != end or not entry.start <= entry.end <= calls:
            raise ExportError("entrypoints must partition every emitted call")
        names.add(entry.name)
        end = entry.end
    if end != calls or len(entries) > 65536:
        raise ExportError("entrypoints must cover every emitted call")
    return 2


@dataclass(frozen=True, slots=True)
class Bindings:
    inputs: Mapping[str, Tensor]
    outputs: Mapping[str, Tensor]


@dataclass(frozen=True, slots=True)
class Allocation:
    bytes: int
    weight_offset: int | None


@dataclass(frozen=True, slots=True)
class View:
    allocation: int
    offset: int
    bytes: int


@dataclass(frozen=True, slots=True)
class Binding:
    name: str
    view: int


@dataclass(frozen=True, slots=True)
class Kernel:
    buffers: int
    scalars: int


@dataclass(frozen=True, slots=True)
class Call:
    kernel: int
    views: tuple[int, ...]
    scalars: tuple[int, ...]
    workers: int
    core_id: int | None


@dataclass(frozen=True, slots=True)
class CapturedModel:
    allocations: tuple[Allocation, ...]
    views: tuple[View, ...]
    inputs: tuple[Binding, ...]
    outputs: tuple[Binding, ...]
    kernels: tuple[Kernel, ...]
    calls: tuple[Call, ...]
    sources: tuple[str, ...]
    objects: tuple[bytes, ...]
    wrappers: tuple[str, ...]
    weights: bytes
    backend: str


@dataclass(frozen=True, slots=True)
class Manifest:
    version: int
    backend: str
    arch: str
    weights_sha256: str
    library_sha256: str
    allocations: tuple[Allocation, ...]
    views: tuple[View, ...]
    inputs: tuple[Binding, ...]
    outputs: tuple[Binding, ...]
    kernels: tuple[Kernel, ...]
    calls: tuple[Call, ...]
    entrypoints: tuple[Entrypoint, ...]
