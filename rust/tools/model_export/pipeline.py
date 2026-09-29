from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass

import numpy as np
from tinygrad import Tensor, TinyJit
from tinygrad.device import Buffer
from tinygrad.dtype import DType, _to_np_dtype
from tinygrad.engine.jit import CapturedJit, _prepare_jit_inputs
from tinygrad.engine.realize import resolve_params
from tinygrad.uop.ops import Ops, UOp

from .binding_order import ordered_inputs
from .schema import Bindings, Entrypoint, ExportError, entrypoint_version


@dataclass(frozen=True, slots=True)
class Stage:
    name: str
    jit: TinyJit
    inputs: Mapping[str, Tensor]


@dataclass(frozen=True, slots=True)
class Pipeline:
    jit: TinyJit
    entrypoints: tuple[Entrypoint, ...]


def linear_calls(linear: UOp) -> tuple[UOp, ...]:
    pending = list(reversed(linear.src))
    calls = []
    visited = 0
    while pending:
        call = pending.pop()
        visited += 1
        if visited > 1_000_000 or call.op is not Ops.CALL or not call.src:
            raise ExportError("invalid or excessive pipeline calls")
        operation = call.src[0]
        if operation.op is Ops.CUSTOM_FUNCTION and operation.arg == "graph":
            if len(operation.src) != 1 or operation.src[0].op is not Ops.LINEAR:
                raise ExportError("invalid captured graph body")
            pending.extend(reversed(operation.src[0].src))
        else:
            calls.append(call)
    return tuple(calls)


def restore_inputs(jit: TinyJit, overrides: Mapping[str, Tensor] | None = None) -> Mapping[str, Tensor]:
    """Recreate static captured input views without relying on their erased base shapes."""
    captured = jit.captured
    if captured is None or len(captured.expected_names) != len(captured.expected_input_info):
        raise ExportError("flat captured input contract required")
    parameters: dict[int, tuple[int, DType, str]] = {}
    for call in linear_calls(captured.linear):
        for argument in call.src[1:]:
            for parameter in argument.toposort():
                if parameter.op is Ops.PARAM:
                    count = parameter.max_numel()
                    if type(count) is not int or not isinstance(parameter.device, str):
                        raise ExportError("static single-device input contract required")
                    contract = (count, parameter.dtype, parameter.device)
                    slot = parameter.arg.slot
                    if slot in parameters and parameters[slot] != contract:
                        raise ExportError("conflicting parameter input contract")
                    parameters[slot] = contract
    supplied = {} if overrides is None else overrides
    names = [str(name) for name in captured.expected_names]
    if len(set(names)) != len(names) or not set(supplied).issubset(names):
        raise ExportError("unknown or ambiguous input override")
    if set(parameters) != set(range(len(names))):
        raise ExportError("missing parameter input contract")
    tensors: dict[str, Tensor] = {}
    for slot, (name, (view, variables, dtype, device)) in enumerate(zip(names, captured.expected_input_info, strict=True)):
        count, parameter_dtype, parameter_device = parameters[slot]
        if variables or dtype != parameter_dtype or device != parameter_device or not 0 < count * dtype.itemsize <= 4 * 1024**3:
            raise ExportError("unsupported parameter input contract")
        if name in supplied:
            if supplied[name].uop.base.buffer.nbytes != count * dtype.itemsize:
                raise ExportError("override buffer size disagrees with captured input contract")
            tensors[name] = supplied[name]
            continue
        base = Tensor(np.zeros(count, dtype=_to_np_dtype(dtype)), device=device).realize().uop.base
        nodes = {node: base for node in view.toposort() if node.op is Ops.NOOP}
        if len(nodes) != 1:
            raise ExportError("input contract requires one erased base")
        tensors[name] = Tensor(view.substitute(nodes))
    _, variables, _, info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, tensors), {})
    if variables or info != captured.expected_input_info:
        raise ExportError("restored tensors disagree with captured input contract")
    return tensors


def compose(stages: Sequence[Stage], bindings: Bindings) -> Pipeline:
    """Resolve original calls to shared concrete buffers, retaining stage boundaries."""
    calls: list[UOp] = []
    entries: list[Entrypoint] = []
    emitted = 0
    for stage in stages:
        captured = stage.jit.captured
        if captured is None:
            raise ExportError("pipeline stage must be captured")
        inputs, variables, _, info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, stage.inputs), {})
        if variables or info != captured.expected_input_info:
            raise ExportError("stage bindings disagree with captured input contract")
        start = emitted
        for call in linear_calls(captured.linear):
            if call.op is not Ops.CALL or call.src[0].op not in (Ops.PROGRAM, Ops.COPY, Ops.SLICE):
                raise ExportError("unsupported pipeline call")
            if any(argument.op is Ops.BIND for argument in call.src[1:]):
                raise ExportError("symbolic pipeline call requires a runtime contract")
            for argument in call.src[1:]:
                for parameter in argument.toposort():
                    if parameter.op is Ops.PARAM and parameter.max_numel() * parameter.dtype.itemsize != inputs[parameter.arg.slot].buffer.nbytes:
                        raise ExportError("stage buffer size disagrees with captured input contract")
            concrete = []
            for argument in resolve_params(call, tuple(inputs)):
                buffer = argument.buffer
                if not isinstance(buffer, Buffer):
                    raise ExportError("single-device pipeline buffers required")
                concrete.append(UOp.from_buffer(buffer))
            calls.append(call.replace(src=(call.src[0], *concrete)))
            emitted += call.src[0].op is not Ops.SLICE
        entries.append(Entrypoint(stage.name, start, emitted))
    if not entries:
        raise ExportError("pipeline requires at least one stage")
    entrypoint_version(tuple(entries), emitted)
    _, variables, names, info = _prepare_jit_inputs((), dict(bindings.inputs))
    if variables:
        raise ExportError("static pipeline inputs required")
    captured = CapturedJit(tuple(bindings.outputs.values()), UOp(Ops.LINEAR, src=tuple(calls)), names, info)
    return Pipeline(TinyJit(None, captured), tuple(entries))
