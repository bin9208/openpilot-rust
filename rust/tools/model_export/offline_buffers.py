from __future__ import annotations

from collections.abc import Mapping

from tinygrad.device import Buffer
from tinygrad.uop.ops import UOp

from .buffers import BufferTable
from .schema import ExportError


def descriptor(buffer: UOp) -> Buffer:
    value = buffer.buffer
    if not isinstance(value, Buffer) or value.device.split(":")[0] not in {"QCOM", "CPU", "NPY", "PYTHON"}:
        raise ExportError("single-device QCOM or host storage required")
    if value.device == "QCOM" and value.is_allocated():
        raise ExportError("offline export requires unallocated QCOM descriptors")
    return value


class OfflineBufferTable(BufferTable):
    """Track source alias/write ranges using genuine initializer bytes without GPU allocation."""

    def __init__(self, initialized: Mapping[Buffer, bytes]) -> None:
        super().__init__()
        self.initialized = initialized

    def read(self, buffer: Buffer) -> None:
        view = self.views[self.view(buffer)]
        remaining = [(view.offset, view.offset + view.bytes)]
        for start, end in self.written[view.allocation]:
            remaining = [(a, b) for low, high in remaining for a, b in ((low, min(high, start)), (max(low, end), high)) if a < b]
        if not remaining:
            return
        data = self.initialized.get(buffer.base)
        if data is None or len(data) != buffer.base.nbytes:
            raise ExportError("offline model reads storage without real initialized bytes")
        for low, high in remaining:
            self.initial[view.allocation][low:high] = data[low:high]
