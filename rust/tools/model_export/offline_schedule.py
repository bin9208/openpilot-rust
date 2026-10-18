from __future__ import annotations

import hashlib
import json
import re
import struct
from contextlib import contextmanager
from dataclasses import asdict, dataclass
from pathlib import Path
from collections.abc import Generator
from typing import TYPE_CHECKING

from tinygrad import Context, Tensor
from tinygrad.codegen import to_program
from tinygrad.dtype import ImageDType
from tinygrad.helpers import Target
from tinygrad.renderer.cstyle import OpenCLRenderer, QCOMCLRenderer
from tinygrad.schedule import memory
from tinygrad.uop.ops import Ops, ProgramInfo, UOp

from .offline_buffers import OfflineBufferTable, descriptor
from .offline_capability import static_movement_capability
from .offline_compiler import OfflineQcomCompiler
from .offline_constants import prepare_constants
from .offline_devices import host_build
from .offline_model import InitializedBuffer as InitializedBuffer, OfflineModel
from .qcom import BufferArgument, CopyCall, ImageArgument, Kernel, KernelCall, Manifest
from .schema import Binding, Entrypoint, ExportError, entrypoint_version

if TYPE_CHECKING:
    from .offline_pipeline import OfflinePipeline


@dataclass(frozen=True, slots=True)
class OfflineCompilation:
    manifest: Manifest
    programs: tuple[UOp, ...]
    weights: bytes


@dataclass(frozen=True, slots=True)
class Emission:
    model: OfflineModel
    linear: UOp
    stage_ends: tuple[tuple[str, int], ...] = ()


class OfflineRenderer(QCOMCLRenderer):
    def __init__(self, compiler: OfflineQcomCompiler) -> None:
        OpenCLRenderer.__init__(self, Target(device="QCOM", arch="a630,IMAGE_PITCH_ALIGNMENT=64"))
        self.compiler = compiler


@contextmanager
def static_allocator_capability() -> Generator[None, None, None]:
    original = memory._can_plan

    def supports_offset(buffer: UOp, held: set[UOp]) -> bool:
        if buffer in held:
            return False
        if buffer.device == "QCOM":
            return True
        if buffer.device not in {"CPU", "NPY", "PYTHON"}:
            raise ExportError("unsupported offline planning device")
        return original(buffer, held)

    memory._can_plan = supports_offset
    try:
        yield
    finally:
        memory._can_plan = original


def plan_linear(linear: UOp, held: set[UOp]) -> UOp:
    with static_allocator_capability():
        return memory.memory_plan_rewrite(linear, held)


def schedule_outputs(outputs: tuple[Tensor, ...], *, float16: int = 1) -> UOp:
    with static_movement_capability(), Context(NO_MEMORY_PLANNER=1, IMAGE=1, FLOAT16=float16, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1):
        linear, variables = outputs[0].linear_with_vars(*outputs[1:])
    if variables:
        raise ExportError("offline model requires static dimensions")
    return linear


def launch_dimensions(info: ProgramInfo, source: str, binary: bytes) -> tuple[tuple[int | float, ...], tuple[int, ...]]:
    """Resolve missing offline locals without changing a kernel's work-item domain."""
    if info.local_size is not None:
        return info.global_size, info.local_size
    if len(info.global_size) != 3 or any(type(size) is not int or not 1 <= size <= 2**32 - 1 for size in info.global_size):
        raise ExportError("unsupported offline QCOM launch dimensions")
    if re.search(r"\b(get_local_id|get_group_id|get_local_size|get_num_groups|barrier|work_group_barrier|__local)\b", source):
        return info.global_size, (1, 1, 1)
    try:
        registers = struct.unpack_from("<I", binary, 0x34)[0]
        fregs, hregs = struct.unpack_from("<II", binary, registers + 0x14)
    except struct.error as error:
        raise ExportError("offline QCOM binary lacks register metadata") from error
    if registers < 0x38 or fregs > 63 or hregs > 63:
        raise ExportError("unsupported offline QCOM register metadata")
    # Match QCOMProgram's thread limit using native-valid register counts.
    max_threads = min(1024, (384 * 32 // (max(1, fregs + (hregs + 1) // 2) * 128)) * 128)
    remaining = min(64, max_threads)
    local = []
    for size in info.global_size:
        group = 1
        while group * 2 <= remaining and size % (group * 2) == 0:
            group *= 2
        local.append(group)
        remaining //= group
    # Like optimize_local_size, globals become group counts, using only exact divisors here.
    return tuple(size // group for size, group in zip(info.global_size, local, strict=True)), tuple(local)


def emit_schedule(schedule: Emission, compiler: OfflineQcomCompiler, destination: Path, *, float16: int = 1) -> OfflineCompilation:
    model = schedule.model
    outputs = tuple(model.bindings.outputs.values())
    if not outputs:
        raise ExportError("offline model needs outputs")
    linear = schedule.linear
    held = {tensor.uop.buf_uop for tensor in (*model.bindings.inputs.values(), *outputs)}
    held.update(item.buffer.buf_uop for item in model.initialized)
    linear = plan_linear(linear, held)
    table = OfflineBufferTable({descriptor(item.buffer): item.data for item in model.initialized})
    inputs = []
    for name, tensor in model.bindings.inputs.items():
        buffer = descriptor(tensor.uop)
        inputs.append(Binding(name, table.view(buffer)))
        if buffer.base in table.initialized:
            table.read(buffer)
        table.write(buffer)
    result_bindings = tuple(Binding(name, table.view(descriptor(tensor.uop))) for name, tensor in model.bindings.outputs.items())
    kernels: list[Kernel] = []
    calls: list[KernelCall | CopyCall] = []
    binaries: list[bytes] = []
    sources: list[str] = []
    programs: list[UOp] = []
    emitted_by_source = [0]
    kernel_ids: dict[Kernel, int] = {}
    renderer = OfflineRenderer(compiler)
    for call in linear.src:
        if call.op is not Ops.CALL:
            raise ExportError("unsupported offline scheduled operation")
        buffers = [descriptor(argument) for argument in call.src[1:]]
        if call.src[0].op is Ops.COPY:
            if len(buffers) != 2 or buffers[0].nbytes != buffers[1].nbytes:
                raise ExportError("offline copy requires equally sized buffers")
            table.read(buffers[1])
            table.write(buffers[0])
            calls.append(CopyCall(table.view(buffers[1]), table.view(buffers[0])))
            emitted_by_source.append(len(calls))
            continue
        if call.src[0].op is not Ops.SINK:
            raise ExportError("unsupported offline scheduled call")
        with Context(IMAGE=1, FLOAT16=float16, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1):
            program = to_program(call.src[0], renderer)
        info = program.arg
        if info.vars or len(info.aux) != 1:
            raise ExportError("unsupported offline kernel argument contract")
        if any(buffers[index].device != "QCOM" for index in info.globals):
            raise ExportError("offline kernel requires QCOM storage")
        arguments = tuple(tuple(ImageArgument(dtype.shape[1], dtype.shape[0], dtype.pitch, dtype.itemsize)
                                if isinstance(dtype, ImageDType) else BufferArgument() for _, dtype in info.aux[0][index])
                          for index in range(len(info.globals)))
        source, binary = program.src[3].arg, program.src[4].arg
        if not isinstance(source, str) or not isinstance(binary, bytes):
            raise ExportError("offline kernel lacks genuine source or compiled bytes")
        global_size, local_size = launch_dimensions(info, source, binary)
        for index in info.ins:
            table.read(buffers[index])
        for index in info.outs:
            table.write(buffers[index])
        kernel = Kernel(info.function_name, hashlib.sha256(binary).hexdigest(), len(binary), arguments)
        if kernel not in kernel_ids:
            kernel_ids[kernel] = len(kernels)
            kernels.append(kernel)
            programs.append(program)
            binaries.append(binary)
            sources.append(source)
        calls.append(KernelCall(kernel_ids[kernel], tuple(table.view(buffers[index]) for index in info.globals), (),
                                global_size, local_size))
        emitted_by_source.append(len(calls))
    for tensor in outputs:
        table.read(descriptor(tensor.uop))
    allocations, weights = table.weights()
    entries = []
    start = 0
    for name, end in schedule.stage_ends:
        entries.append(Entrypoint(name, start, emitted_by_source[end]))
        start = emitted_by_source[end]
    manifest = Manifest(entrypoint_version(tuple(entries), len(calls)), "qcom-cl", "a630", hashlib.sha256(weights).hexdigest(),
                        allocations, tuple(table.views), tuple(inputs), result_bindings, tuple(kernels), tuple(calls), tuple(entries))
    serialized = asdict(manifest)
    if not entries:
        del serialized["entrypoints"]
    for call_data in serialized["calls"]:
        if "global_" in call_data:
            call_data["global"] = call_data.pop("global_")
    destination.mkdir(parents=True, exist_ok=False)
    for index, (source, binary) in enumerate(zip(sources, binaries, strict=True)):
        (destination / f"kernel-{index}.cl").write_text(source)
        (destination / f"kernel-{index}.bin").write_bytes(binary)
    (destination / "weights.bin").write_bytes(weights)
    (destination / "graph.json").write_text(json.dumps(serialized, indent=2) + "\n")
    return OfflineCompilation(manifest, tuple(programs), weights)


@host_build
def export_model(model: OfflineModel, compiler: OfflineQcomCompiler, destination: Path, *, float16: int = 1) -> OfflineCompilation:
    if not model.bindings.outputs:
        raise ExportError("offline model needs outputs")
    prepared = prepare_constants(model, schedule_outputs(tuple(model.bindings.outputs.values()), float16=float16))
    return emit_schedule(Emission(OfflineModel(model.bindings, prepared.initialized), prepared.linear), compiler, destination, float16=float16)


@host_build
def export_pipeline(pipeline: OfflinePipeline, compiler: OfflineQcomCompiler, destination: Path) -> OfflineCompilation:
    linear = UOp(Ops.LINEAR, src=tuple(call for stage in pipeline.stages for call in stage.linear.src))
    prepared = prepare_constants(pipeline.model, linear)
    retained = set(prepared.linear.src)
    ends = []
    end = 0
    for stage in pipeline.stages:
        end += sum(call in retained for call in stage.linear.src)
        ends.append((stage.name, end))
    model = OfflineModel(pipeline.model.bindings, prepared.initialized)
    return emit_schedule(Emission(model, prepared.linear, tuple(ends)), compiler, destination)
