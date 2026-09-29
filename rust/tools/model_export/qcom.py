from __future__ import annotations

import hashlib
import json
import math
from dataclasses import asdict, dataclass
from pathlib import Path

from tinygrad import TinyJit
from tinygrad.device import Buffer, Device, MultiBuffer
from tinygrad.dtype import ImageDType
from tinygrad.engine.jit import _prepare_jit_inputs
from tinygrad.engine.realize import resolve_params
from tinygrad.renderer.cstyle import QCOMCLRenderer
from tinygrad.uop.ops import Ops

from .buffers import BufferTable
from .binding_order import ordered_inputs
from .schema import Allocation, Binding, Bindings, ExportError, View


@dataclass(frozen=True, slots=True)
class BufferArgument:
    kind: str = "buffer"


@dataclass(frozen=True, slots=True)
class ImageArgument:
    width: int
    height: int
    pitch: int
    element_bytes: int
    kind: str = "image"


@dataclass(frozen=True, slots=True)
class Kernel:
    name: str
    binary_sha256: str
    binary_bytes: int
    arguments: tuple[tuple[BufferArgument | ImageArgument, ...], ...]


@dataclass(frozen=True, slots=True)
class KernelCall:
    kernel: int
    views: tuple[int, ...]
    scalars: tuple[int, ...]
    global_: tuple[int | float, ...]
    local: tuple[int, ...]
    op: str = "kernel"


@dataclass(frozen=True, slots=True)
class CopyCall:
    source: int
    destination: int
    op: str = "copy"


@dataclass(frozen=True, slots=True)
class Manifest:
    version: int
    backend: str
    arch: str
    weights_sha256: str
    allocations: tuple[Allocation, ...]
    views: tuple[View, ...]
    inputs: tuple[Binding, ...]
    outputs: tuple[Binding, ...]
    kernels: tuple[Kernel, ...]
    calls: tuple[KernelCall | CopyCall, ...]


def qcom_buffer(buffer: Buffer | MultiBuffer) -> Buffer:
    if not isinstance(buffer, Buffer) or buffer.device.split(":")[0] not in {"QCOM", "CPU", "NPY", "PYTHON"}:
        raise ExportError("QCOM export requires single-device QCOM or host buffers")
    return buffer


def export_qcom(jit: TinyJit, bindings: Bindings, destination: Path) -> None:
    captured = jit.captured
    if captured is None:
        raise ExportError("JIT must be captured before export")
    if len(captured.expected_names) != len(captured.expected_input_info) or len(bindings.inputs) != len(captured.expected_names):
        raise ExportError("QCOM export requires flat bindings for every captured input")
    input_uops, variables, _, input_info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, bindings.inputs), {})
    if variables or input_info != captured.expected_input_info:
        raise ExportError("bindings disagree with the captured input contract")
    inputs = tuple(input_uops)
    for call in captured.linear.src:
        for argument in call.src[1:]:
            for parameter in argument.toposort():
                if parameter.op is Ops.PARAM:
                    buffer = qcom_buffer(inputs[parameter.arg.slot].buffer)
                    if parameter.max_numel() * parameter.dtype.itemsize != buffer.nbytes:
                        raise ExportError("buffer size disagrees with the captured input contract")
    table = BufferTable()
    input_bindings = []
    for name, tensor in bindings.inputs.items():
        buffer = qcom_buffer(tensor.uop.base.buffer)
        if buffer.nbytes != tensor.numel() * tensor.dtype.itemsize:
            raise ExportError("input bindings must cover their complete base buffer")
        table.read(buffer)
        input_bindings.append(Binding(name, table.view(buffer)))
    output_bindings = []
    for name, tensor in bindings.outputs.items():
        buffer = qcom_buffer(tensor.uop.buffer)
        table.read(buffer)
        output_bindings.append(Binding(name, table.view(buffer)))
    kernels: list[Kernel] = []
    binaries: list[bytes] = []
    sources: list[str] = []
    calls: list[KernelCall | CopyCall] = []
    kernel_ids: dict[Kernel, int] = {}
    for call in captured.linear.src:
        if call.op is not Ops.CALL:
            raise ExportError(f"unsupported captured operation: {call.op}")
        operation = call.src[0]
        buffers = [qcom_buffer(uop.buffer) for uop in resolve_params(call, inputs)]
        if operation.op is Ops.SLICE:
            if len(buffers) != 2 or buffers[0].base is not buffers[1].base:
                raise ExportError("slice must preserve backing allocation")
            table.view(buffers[0])
            continue
        if operation.op is Ops.COPY:
            if len(buffers) != 2 or buffers[0].nbytes != buffers[1].nbytes:
                raise ExportError("copy requires equally sized source and destination")
            table.read(buffers[1])
            table.write(buffers[0])
            calls.append(CopyCall(table.view(buffers[1]), table.view(buffers[0])))
            continue
        if operation.op is not Ops.PROGRAM:
            raise ExportError(f"unsupported captured call: {operation.op}")
        info = operation.arg
        renderer = Device[operation.src[1].arg].renderer
        if not isinstance(renderer, QCOMCLRenderer) or renderer.target.arch.split(",")[0] != "a630":
            raise ExportError("QCOMCL a630 renderer required")
        if any(buffers[index].device.split(":")[0] != "QCOM" for index in info.globals):
            raise ExportError("QCOM kernel requires QCOM storage")
        if info.vars:
            raise ExportError("symbolic kernel scalars require an explicit runtime contract")
        local = info.local_size or (1, 1, 1)
        if (len(info.global_size) != 3 or len(local) != 3 or any(type(dim) is not int or dim <= 0 for dim in local) or
            any(type(dim) not in (int, float) or not math.isfinite(dim) or dim <= 0 for dim in info.global_size)):
            raise ExportError("unsupported QCOM launch dimensions")
        if len(info.aux) != 1 or len(info.aux[0]) < len(info.globals):
            raise ExportError("QCOM kernel has no argument type contract")
        arguments = tuple(tuple(ImageArgument(dtype.shape[1], dtype.shape[0], dtype.pitch, dtype.itemsize)
                                if isinstance(dtype, ImageDType) else BufferArgument() for _, dtype in info.aux[0][index])
                          for index in range(len(info.globals)))
        binary, source = operation.src[4].arg, operation.src[3].arg
        if not isinstance(binary, bytes) or not isinstance(source, str):
            raise ExportError("QCOM program requires source and compiled bytes")
        kernel = Kernel(info.function_name, hashlib.sha256(binary).hexdigest(), len(binary), arguments)
        if kernel not in kernel_ids:
            kernel_ids[kernel] = len(kernels)
            kernels.append(kernel)
            binaries.append(binary)
            sources.append(source)
        for index in info.ins:
            table.read(buffers[index])
        for index in info.outs:
            table.write(buffers[index])
        calls.append(KernelCall(kernel_ids[kernel], tuple(table.view(buffers[index]) for index in info.globals), (), info.global_size, local))
    allocations, weights = table.weights()
    manifest = Manifest(1, "qcom-cl", "a630", hashlib.sha256(weights).hexdigest(), allocations, tuple(table.views),
                        tuple(input_bindings), tuple(output_bindings), tuple(kernels), tuple(calls))
    serialized = asdict(manifest)
    for call in serialized["calls"]:
        if "global_" in call:
            call["global"] = call.pop("global_")
    destination.mkdir(parents=True, exist_ok=False)
    for index, (binary, source) in enumerate(zip(binaries, sources, strict=True)):
        (destination / f"kernel-{index}.bin").write_bytes(binary)
        (destination / f"kernel-{index}.cl").write_text(source)
    (destination / "weights.bin").write_bytes(weights)
    (destination / "graph.json").write_text(json.dumps(serialized, indent=2) + "\n")
