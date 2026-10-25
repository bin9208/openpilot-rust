from __future__ import annotations

import hashlib
import json
import math
import os
import re
import secrets
import selectors
import signal
import struct
import subprocess
import time
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
from tinygrad.uop.ops import Ops, UOp

from .offline_buffers import descriptor
from .offline_capability import static_movement_capability
from .offline_compiler import OfflineQcomCompiler
from .offline_constants import prepare_constants
from .offline_devices import host_build
from .offline_model import InitializedBuffer as InitializedBuffer, OfflineModel
from .qcom import BufferArgument, CopyCall, ImageArgument, Kernel, KernelCall, Manifest
from .schema import Allocation, Binding, Entrypoint, ExportError, View

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
    # Keep genuine pre-pruning CALL objects alive, including repeated occurrences.
    stages: tuple[tuple[str, tuple[UOp, ...]], ...] = ()


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


def schedule_outputs(outputs: tuple[Tensor, ...]) -> UOp:
    with static_movement_capability(), Context(NO_MEMORY_PLANNER=1, IMAGE=1, FLOAT16=1, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1):
        linear, variables = outputs[0].linear_with_vars(*outputs[1:])
    if variables:
        raise ExportError("offline model requires static dimensions")
    return linear


def _number(value: int | float | bool) -> dict:
    if type(value) is int:
        return {"type": "integer", "value": str(value)}
    if type(value) is float:
        return {"type": "float", "value": value}
    if type(value) is bool:
        return {"type": "boolean", "value": value}
    raise ExportError("unsupported native export dimension")


class _NativeExportStream:
    """One owned native process; synchronous metadata checkpoints, no Python worker."""
    _limit = 256 * 1024 * 1024
    _frames = 1_048_576 + 2 * 65536 + 2

    def __init__(self, compiler: OfflineQcomCompiler, initialized: dict) -> None:
        self.compiler, self.initialized = compiler, initialized
        self.session, self.sequence = secrets.token_hex(16), 0
        self.failed = self.finished = self.stderr_eof = self.stdout_eof = False
        self.sent = self.received = 0
        self.stderr = bytearray()
        self.programs = {}  # Strong references prevent source-program ID reuse.
        self.process = None

    def __enter__(self):
        if self.process is not None or self.failed or self.finished:
            self.failed = True
            raise ExportError("native export-stream cannot be entered again")
        try:
            self.process = subprocess.Popen((*self.compiler.command.argv, "--export-stream"),
                                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                            bufsize=0, close_fds=True, start_new_session=True)
            for channel in (self.process.stdin, self.process.stdout, self.process.stderr):
                os.set_blocking(channel.fileno(), False)
            items, size = [], 0
            if len(self.initialized) > 65536:
                raise ExportError("native export-stream initializer count exceeds bound")
            for buffer, data in self.initialized.items():
                # Wire admission only: unused/wrong-length data remains lazy.
                size += 2 * len(data)
                if size > self._limit - 4:
                    raise ExportError("native export-stream input exceeds bound")
                items.append({"key": id(buffer), "data": bytes(data).hex()})
            self.request({"op": "begin", "version": 1,
                          "initialized": items})
            return self
        except BaseException as error:
            self.failed = True
            try:
                self._abort()
            except BaseException as cleanup:
                error.add_note("native stream reap failed: " + str(cleanup))
            self._close()
            if isinstance(error, OSError):
                raise ExportError("native export-stream process failed: " + str(error)) from error
            raise

    def _drain_stderr(self):
        while not self.stderr_eof:
            try:
                raw = os.read(self.process.stderr.fileno(), 65536)
            except BlockingIOError:
                return
            if not raw:
                self.stderr_eof = True
                return
            if len(self.stderr) + len(raw) > 65536:
                raise ExportError("native export-stream stderr exceeds bound")
            self.stderr.extend(raw)

    def _ready(self, channel, writing, deadline):
        with selectors.DefaultSelector() as poll:
            poll.register(channel, selectors.EVENT_WRITE if writing else selectors.EVENT_READ)
            if not self.stderr_eof:
                poll.register(self.process.stderr, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ExportError("native export-stream timeout")
                for key, _ in poll.select(remaining):
                    if key.fileobj is self.process.stderr:
                        self._drain_stderr()
                        if self.stderr_eof:
                            poll.unregister(self.process.stderr)
                    else:
                        return

    def _write_all(self, raw, deadline):
        pending = memoryview(raw)
        while pending:
            self._ready(self.process.stdin, True, deadline)
            try:
                count = os.write(self.process.stdin.fileno(), pending[:65536])
            except BlockingIOError:
                continue
            if count == 0:
                raise ExportError("native export-stream write stopped")
            pending = pending[count:]

    def _read_exact(self, length, deadline):
        raw = bytearray()
        while len(raw) < length:
            self._ready(self.process.stdout, False, deadline)
            try:
                part = os.read(self.process.stdout.fileno(), min(65536, length - len(raw)))
            except BlockingIOError:
                continue
            if not part:
                self.stdout_eof = True
                self._drain_stderr()
                raise ExportError("native export-stream failed: " + self.stderr.decode(errors="replace"))
            raw.extend(part)
        return raw

    def request(self, command, final=False):
        if self.failed or self.finished:
            raise ExportError("native export-stream is failed or finished")
        try:
            if self.sequence >= self._frames:
                raise ExportError("native export-stream frame count exceeds bound")
            raw = bytearray()
            encoder = json.JSONEncoder(separators=(",", ":"), allow_nan=False)
            for part in encoder.iterencode({"session": self.session, "sequence": self.sequence, "command": command}):
                # ensure_ascii=True makes character count the exact wire size.
                if self.sent + 4 + len(raw) + len(part) > self._limit:
                    raise ExportError("native export-stream input exceeds bound")
                raw.extend(part.encode("ascii"))
            self.sent += 4 + len(raw)
            deadline = time.monotonic() + self.compiler.command.timeout_seconds
            self._write_all(struct.pack(">I", len(raw)), deadline)
            self._write_all(raw, deadline)
            if final:
                self.process.stdin.close()  # Native withholds final until EOF.
            length = struct.unpack(">I", self._read_exact(4, deadline))[0]
            self.received += 4 + length
            if length == 0 or self.received > self._limit:
                raise ExportError("native export-stream output exceeds bound")
            def unique(pairs):
                value = {}
                for name, item in pairs:
                    if name in value:
                        raise ValueError("duplicate stream field")
                    value[name] = item
                return value
            def nonfinite(value):
                raise ValueError("nonfinite stream number: " + value)
            reply = json.loads(self._read_exact(length, deadline), object_pairs_hook=unique, parse_constant=nonfinite)
            expected = {"kind", "session", "sequence", "complete"} | ({"plan"} if final else set())
            if (type(reply) is not dict or set(reply) != expected or reply["session"] != self.session
                    or type(reply["sequence"]) is not int or reply["sequence"] != self.sequence
                    or reply["kind"] != ("final" if final else "checkpoint") or reply["complete"] is not final):
                raise ExportError("native export-stream response contract differs")
            self.sequence += 1
            self.finished = final
            return reply.get("plan")
        except BaseException as error:
            self.failed = True
            if isinstance(error, (OSError, ValueError, struct.error)):
                self._drain_stderr()
                raise ExportError("native export-stream failed: " + self.stderr.decode(errors="replace") + str(error)) from error
            raise

    def event(self, event):
        self.request({"op": "event", "event": event})

    def launch(self, program, info, source, binary):
        key = id(program)
        if key not in self.programs:
            if len(source) + 2 * len(binary) > self._limit - 4:
                self.failed = True
                raise ExportError("native export-stream program exceeds wire bound")
            self.programs[key] = program
            self.request({"op": "program", "key": key, "program": {"source": source, "binary": binary.hex()}})
        self.request({"op": "launch", "key": key, "global": [_number(value) for value in info.global_size],
                      "local": None if info.local_size is None else [_number(value) for value in info.local_size]})

    def finish(self, request):
        return self.request({"op": "finish", "emitted_by_source": request["emitted_by_source"],
                             "stages": request["stages"], "retained_calls": request["retained_calls"]}, final=True)

    def _abort(self):
        if self.process is not None and self.process.returncode is None:
            # No poll/reap before killpg: the owned unreaped child reserves PID.
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.process.wait(timeout=5)

    def _close(self):
        if self.process is not None:
            for channel in (self.process.stdin, self.process.stdout, self.process.stderr):
                channel.close()

    def __exit__(self, kind, error, traceback):
        try:
            if error is not None or self.failed or not self.finished:
                self._abort()
                if error is None:
                    raise ExportError("native export-stream was not finalized")
            else:
                try:
                    deadline = time.monotonic() + self.compiler.command.timeout_seconds
                    while not (self.stdout_eof and self.stderr_eof):
                        if not self.stdout_eof:
                            self._ready(self.process.stdout, False, deadline)
                            try:
                                raw = os.read(self.process.stdout.fileno(), 1)
                            except BlockingIOError:
                                continue
                            if raw:
                                raise ExportError("trailing native export-stream output")
                            self.stdout_eof = True
                        if not self.stderr_eof:
                            # The native process is exiting; stderr must also close.
                            with selectors.DefaultSelector() as poll:
                                poll.register(self.process.stderr, selectors.EVENT_READ)
                                remaining = deadline - time.monotonic()
                                if remaining <= 0 or not poll.select(remaining):
                                    raise ExportError("native export-stream EOF timeout")
                            self._drain_stderr()
                    if self.process.wait(timeout=5) != 0 or self.stderr:
                        raise ExportError("native export-stream final process failed: " + self.stderr.decode(errors="replace"))
                except BaseException:
                    self._abort()
                    raise
        finally:
            self._close()
        return False


def _decode_native_plan(native: dict, request: dict) -> dict:
    """Validate wire shape/references only; native owns interning, ranges and geometry."""
    def fields(value, names):
        if type(value) is not dict or set(value) != set(names):
            raise ExportError("invalid native export fields")
        return value

    def array(value, limit):
        if type(value) is not list or len(value) > limit:
            raise ExportError("invalid native export array")
        return value

    def natural(value, limit):
        if type(value) is not int or not 0 <= value <= limit:
            raise ExportError("invalid native export index/extent")
        return value

    def dimension(value):
        fields(value, ("type", "value"))
        tag, number = value["type"], value["value"]
        if tag == "integer" and type(number) is str and len(number) <= 40 and re.fullmatch(r"-?(0|[1-9][0-9]*)", number):
            number = int(number)
            if -(2**127) <= number < 2**127:
                return number
        elif tag == "float" and type(number) is float and math.isfinite(number):
            return number
        elif tag == "boolean" and type(number) is bool:
            return number
        raise ExportError("invalid native export dimension")

    fields(native, ("version", "allocations", "views", "event_views", "weights", "weights_sha256", "launches", "entrypoints"))
    if type(native["version"]) is not int or native["version"] not in (1, 2):
        raise ExportError("invalid native graph version")
    if (native["version"] == 1) != (not request["stages"]):
        raise ExportError("native graph version differs from stage presence")
    raw_weights = native["weights"]
    if (type(raw_weights) is not str or len(raw_weights) > 256 * 1024 * 1024
            or len(raw_weights) % 2 or re.fullmatch(r"[0-9a-f]*", raw_weights) is None):
        raise ExportError("invalid native weight bytes")
    weights = bytes.fromhex(raw_weights)
    if native["weights_sha256"] != hashlib.sha256(weights).hexdigest():
        raise ExportError("native weight digest differs")
    allocations = []
    for value in array(native["allocations"], len(request["events"])):
        fields(value, ("bytes", "weight_offset"))
        size = natural(value["bytes"], 1024**3)
        offset = value["weight_offset"]
        if offset is not None and natural(offset, len(weights)) + size > len(weights):
            raise ExportError("native allocation exceeds weight bytes")
        allocations.append(Allocation(size, offset))
    if sum(value.bytes for value in allocations) > 1024**3:
        raise ExportError("native allocations exceed envelope")
    views = []
    for value in array(native["views"], len(request["events"])):
        fields(value, ("allocation", "offset", "bytes"))
        allocation = natural(value["allocation"], len(allocations) - 1)
        size = allocations[allocation].bytes
        offset, length = natural(value["offset"], size), natural(value["bytes"], size)
        if offset + length > size:
            raise ExportError("native view exceeds allocation")
        views.append(View(allocation, offset, length))
    event_views = array(native["event_views"], len(request["events"]))
    if len(event_views) != len(request["events"]):
        raise ExportError("native event count differs")
    for index, event in zip(event_views, request["events"], strict=True):
        view = views[natural(index, len(views) - 1)]
        storage = event["buffer"]
        if (view.offset != storage["offset"] or view.bytes != storage["bytes"]
                or allocations[view.allocation].bytes != storage["allocation_bytes"]):
            raise ExportError("native event metadata differs")
    launches = []
    raw_launches = array(native["launches"], len(request["launches"]))
    if len(raw_launches) != len(request["launches"]):
        raise ExportError("native launch count differs")
    for launch, supplied in zip(raw_launches, request["launches"], strict=True):
        fields(launch, ("global", "local"))
        lengths = (len(supplied["global"]), 3 if supplied["local"] is None else len(supplied["local"]))
        sizes = []
        for name, length in zip(("global", "local"), lengths, strict=True):
            values = array(launch[name], length)
            if len(values) != length:
                raise ExportError("native launch rank differs")
            sizes.append(tuple(dimension(value) for value in values))
        launches.append(tuple(sizes))
    entries = []
    raw_entries = array(native["entrypoints"], len(request["stages"]))
    if len(raw_entries) != len(request["stages"]):
        raise ExportError("native stage count differs")
    for value, stage in zip(raw_entries, request["stages"], strict=True):
        fields(value, ("name", "start", "end"))
        if type(value["name"]) is not str or value["name"] != stage["name"]:
            raise ExportError("native stage name differs")
        start = natural(value["start"], request["emitted_by_source"][-1])
        end = natural(value["end"], request["emitted_by_source"][-1])
        if start > end:
            raise ExportError("invalid native stage range")
        entries.append(Entrypoint(value["name"], start, end))
    return native | {"weights": weights, "allocations": tuple(allocations), "views": tuple(views),
                     "event_views": tuple(event_views), "launches": tuple(launches), "entrypoints": tuple(entries)}


def emit_schedule(schedule: Emission, compiler: OfflineQcomCompiler, destination: Path) -> OfflineCompilation:
    model = schedule.model
    outputs = tuple(model.bindings.outputs.values())
    if not outputs:
        raise ExportError("offline model needs outputs")
    linear = schedule.linear
    retained_calls = tuple(linear.src)
    held = {tensor.uop.buf_uop for tensor in (*model.bindings.inputs.values(), *outputs)}
    held.update(item.buffer.buf_uop for item in model.initialized)
    linear = plan_linear(linear, held)
    if len(linear.src) != len(retained_calls):
        raise ExportError("offline planning changed source-call positions")
    initialized = {descriptor(item.buffer): item.data for item in model.initialized}
    with _NativeExportStream(compiler, initialized) as stream:
        events, launches, storage_lifetimes = [], [], []

        def storage(buffer):
            # The removed Python table used to retain base objects. IDs on the wire
            # must not outlive their genuine objects or be reused during emission.
            storage_lifetimes.append(buffer)
            return {"base": id(buffer.base), "allocation_bytes": buffer.base.nbytes, "offset": buffer.offset,
                    "bytes": buffer.nbytes, "device": buffer.device, "allocated": buffer.is_allocated()}

        def view(buffer):
            event = len(events)
            events.append({"op": "view", "buffer": storage(buffer)})
            stream.event(events[-1])
            return event

        def read(buffer):
            events.append({"op": "read", "buffer": storage(buffer)})
            stream.event(events[-1])

        def write(buffer):
            events.append({"op": "write", "buffer": storage(buffer)})
            stream.event(events[-1])

        inputs = []
        for name, tensor in model.bindings.inputs.items():
            buffer = descriptor(tensor.uop)
            inputs.append(Binding(name, view(buffer)))
            if buffer.base in initialized:
                read(buffer)
            write(buffer)
        result_bindings = tuple(Binding(name, view(descriptor(tensor.uop))) for name, tensor in model.bindings.outputs.items())
        kernels: list[Kernel] = []
        calls: list[tuple[int, tuple[int, ...]] | CopyCall] = []
        binaries: list[bytes] = []
        sources: list[str] = []
        programs: list[UOp] = []
        emitted_by_source = [0]
        kernel_ids: dict[Kernel, int] = {}
        renderer = OfflineRenderer(compiler)
        for original, call in zip(retained_calls, linear.src, strict=True):
            if (call.op is not Ops.CALL or original.op is not Ops.CALL
                    or call.src[0].op is not original.src[0].op):
                raise ExportError("unsupported offline scheduled operation")
            buffers = [descriptor(argument) for argument in call.src[1:]]
            if call.src[0].op is Ops.COPY:
                if len(buffers) != 2 or buffers[0].nbytes != buffers[1].nbytes:
                    raise ExportError("offline copy requires equally sized buffers")
                read(buffers[1])
                write(buffers[0])
                calls.append(CopyCall(view(buffers[1]), view(buffers[0])))
                emitted_by_source.append(len(calls))
                continue
            if call.src[0].op is not Ops.SINK:
                raise ExportError("unsupported offline scheduled call")
            with Context(IMAGE=1, FLOAT16=1, NOLOCALS=1, BEAM=0, OPENPILOT_HACKS=1):
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
            stream.launch(program, info, source, binary)
            for index in info.ins:
                read(buffers[index])
            for index in info.outs:
                write(buffers[index])
            kernel = Kernel(info.function_name, hashlib.sha256(binary).hexdigest(), len(binary), arguments)
            if kernel not in kernel_ids:
                kernel_ids[kernel] = len(kernels)
                kernels.append(kernel)
                programs.append(program)
                binaries.append(binary)
                sources.append(source)
            launches.append({"program": kernel_ids[kernel], "global": [_number(value) for value in info.global_size],
                             "local": None if info.local_size is None else [_number(value) for value in info.local_size]})
            calls.append((kernel_ids[kernel], tuple(view(buffers[index]) for index in info.globals)))
            emitted_by_source.append(len(calls))
        for tensor in outputs:
            read(descriptor(tensor.uop))
        request = {
            "version": 2, "events": events,
            "launches": launches, "emitted_by_source": emitted_by_source,
            "retained_calls": [id(call) for call in retained_calls],
            "stages": [{"name": name, "calls": [id(call) for call in stage]} for name, stage in schedule.stages],
        }
        native = _decode_native_plan(stream.finish(request), request)
    event_views = native["event_views"]
    native_calls = []
    launch_iter = iter(native["launches"])
    for call in calls:
        if isinstance(call, CopyCall):
            native_calls.append(CopyCall(event_views[call.source], event_views[call.destination]))
        else:
            kernel, arguments = call
            global_size, local_size = next(launch_iter)
            native_calls.append(KernelCall(kernel, tuple(event_views[token] for token in arguments), (), global_size, local_size))
    weights = native["weights"]
    manifest = Manifest(native["version"], "qcom-cl", "a630", native["weights_sha256"], native["allocations"], native["views"],
                        tuple(Binding(value.name, event_views[value.view]) for value in inputs),
                        tuple(Binding(value.name, event_views[value.view]) for value in result_bindings),
                        tuple(kernels), tuple(native_calls), native["entrypoints"])
    serialized = asdict(manifest)
    if not native["entrypoints"]:
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
def export_model(model: OfflineModel, compiler: OfflineQcomCompiler, destination: Path) -> OfflineCompilation:
    if not model.bindings.outputs:
        raise ExportError("offline model needs outputs")
    prepared = prepare_constants(model, schedule_outputs(tuple(model.bindings.outputs.values())))
    return emit_schedule(Emission(OfflineModel(model.bindings, prepared.initialized), prepared.linear), compiler, destination)


@host_build
def export_pipeline(pipeline: OfflinePipeline, compiler: OfflineQcomCompiler, destination: Path) -> OfflineCompilation:
    linear = UOp(Ops.LINEAR, src=tuple(call for stage in pipeline.stages for call in stage.linear.src))
    prepared = prepare_constants(pipeline.model, linear)
    model = OfflineModel(pipeline.model.bindings, prepared.initialized)
    stages = tuple((stage.name, tuple(stage.linear.src)) for stage in pipeline.stages)
    return emit_schedule(Emission(model, prepared.linear, stages), compiler, destination)
