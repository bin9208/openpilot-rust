from __future__ import annotations

import struct

from tinygrad import Tensor, dtypes

from model_export import Bindings
from model_export.offline_constants import prepare_constants
from model_export.offline_schedule import InitializedBuffer, OfflineModel, descriptor, schedule_outputs


def test_real_cpu_weight_transform_preserves_half_bits_when_source_prunes_layout() -> None:
    # Given nonzero half weights with a source transpose and zero-padding transform.
    values = (1.25, -2.5, 3.75, 0.125, 11.0, 12.0)
    weight = Tensor.empty(2, 3, dtype=dtypes.float16, device="QCOM")
    transformed = weight.permute(1, 0).pad(((0, 1), (0, 0))).contiguous()
    runtime = Tensor.empty(4, 2, dtype=dtypes.float16, device="QCOM")
    model = OfflineModel(Bindings({"input": runtime}, {"output": runtime + transformed}),
                         (InitializedBuffer(weight.uop.base, struct.pack("<6e", *values)),))
    linear = schedule_outputs(tuple(model.bindings.outputs.values()))

    # When the original pruned constant AST executes on the real CPU backend.
    prepared = prepare_constants(model, linear)

    # Then only input-dependent calls remain, with exact initialized transpose/padding bytes.
    assert prepared.onetime_calls == 1
    assert len(prepared.linear.src) == 1
    expected = struct.pack("<8e", 1.25, 0.125, -2.5, 11.0, 3.75, 12.0, 0.0, 0.0)
    assert expected in tuple(item.data for item in prepared.initialized)
    assert all(len(item.data) == descriptor(item.buffer).nbytes for item in prepared.initialized)
