from __future__ import annotations

from tinygrad.device import Buffer, MultiBuffer

from .schema import Allocation, ExportError, View


def host_buffer(buffer: Buffer | MultiBuffer) -> Buffer:
    if not isinstance(buffer, Buffer) or buffer.device.split(":")[0] not in {"CPU", "NPY", "PYTHON"}:
        raise ExportError("CPU export requires single-device host buffers")
    return buffer


class BufferTable:
    """Accumulates aliased storage and initialized ranges while walking call order."""

    def __init__(self) -> None:
        self.bases: list[Buffer] = []
        self.base_ids: dict[int, int] = {}
        self.view_ids: dict[tuple[int, int, int], int] = {}
        self.views: list[View] = []
        self.initial: list[bytearray] = []
        self.written: list[list[tuple[int, int]]] = []

    def view(self, buffer: Buffer) -> int:
        key = id(buffer.base)
        if key not in self.base_ids:
            self.base_ids[key] = len(self.bases)
            self.bases.append(buffer.base)
            self.initial.append(bytearray(buffer.base.nbytes))
            self.written.append([])
        index = self.base_ids[key]
        view_key = (index, buffer.offset, buffer.nbytes)
        if view_key not in self.view_ids:
            self.view_ids[view_key] = len(self.views)
            self.views.append(View(*view_key))
        return self.view_ids[view_key]

    def read(self, buffer: Buffer) -> None:
        view = self.views[self.view(buffer)]
        remaining = [(view.offset, view.offset + view.bytes)]
        for start, end in self.written[view.allocation]:
            remaining = [(a, b) for low, high in remaining for a, b in ((low, min(high, start)), (max(low, end), high)) if a < b]
        if remaining:
            data = bytearray(buffer.nbytes)
            buffer.ensure_allocated().copyout(memoryview(data))
            for low, high in remaining:
                self.initial[view.allocation][low:high] = data[low - view.offset:high - view.offset]

    def write(self, buffer: Buffer) -> None:
        view = self.views[self.view(buffer)]
        self.written[view.allocation].append((view.offset, view.offset + view.bytes))

    def weights(self) -> tuple[tuple[Allocation, ...], bytes]:
        allocations: list[Allocation] = []
        packed = bytearray()
        for data in self.initial:
            offset = len(packed) if any(data) else None
            allocations.append(Allocation(len(data), offset))
            if offset is not None:
                packed.extend(data)
        return tuple(allocations), bytes(packed)
