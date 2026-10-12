from __future__ import annotations

from collections.abc import Generator
from contextlib import contextmanager

from tinygrad import callify
from tinygrad.helpers import all_int
from tinygrad.uop.ops import Ops, PatternMatcher, UOp

from .schema import ExportError


def contiguous_mops_to_view(contiguous: UOp, source: UOp) -> UOp | None:
    device = contiguous.device
    if device in {"CPU", "PYTHON", "NPY"}:
        return callify.contiguous_mops_to_view(contiguous, source)
    if device != "QCOM":
        raise ExportError("unsupported offline movement device")
    if source.base.op not in {Ops.BUFFER, Ops.SLICE}:
        return None
    if source.op is Ops.RESHAPE and source.src[0].op in {Ops.BUFFER, Ops.SLICE}:
        return None
    if not all_int(contiguous.shape):
        return None
    view = callify._make_buffer_view(source)
    return None if view is None else view.contiguous(tag=contiguous.tag)


def offline_movement(c: UOp, src: UOp) -> UOp | None:
    return contiguous_mops_to_view(c, src)


@contextmanager
def static_movement_capability() -> Generator[None, None, None]:
    original = callify.pm_early_transform_tensor_graph
    callify.pm_early_transform_tensor_graph = PatternMatcher([
        (pattern, offline_movement if function is callify.contiguous_mops_to_view else function)
        for pattern, function in original.patterns
    ])
    try:
        yield
    finally:
        callify.pm_early_transform_tensor_graph = original
