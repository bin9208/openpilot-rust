from __future__ import annotations

import json
import os
import struct
from pathlib import Path

import pytest
from tinygrad import Tensor, dtypes
from tinygrad import callify
from tinygrad.device import Compiled, Device
from tinygrad.helpers import Target
from tinygrad.runtime.ops_python import PythonProgram, PythonRenderer
from tinygrad.schedule.memory import memory_plan_rewrite
from tinygrad.uop.ops import Ops, UOp

from model_export import Bindings
from model_export.buffers import BufferTable
from model_export.offline_compiler import CompilerCommand, OfflineQcomCompiler
from model_export.offline_capability import contiguous_mops_to_view
from model_export.offline_schedule import InitializedBuffer, OfflineModel, export_model, plan_linear, schedule_outputs


def test_offline_plan_matches_original_when_storage_supports_views() -> None:
    # Given a dependency chain whose real CPU allocator supports offsets.
    a, b, c = (UOp.new_buffer("CPU", 64, dtypes.float32) for _ in range(3))
    linear = UOp(Ops.LINEAR, src=(UOp(Ops.COPY).call(b, a), UOp(Ops.COPY).call(c, b)))
    original = memory_plan_rewrite(linear, {a, c})

    # When the same source planner runs with the offline capability seam.
    planned = plan_linear(linear, {a, c})

    # Then buffer lifetimes, offsets, and sizes are identical.
    original_table, planned_table = BufferTable(), BufferTable()
    for before, after in zip(original.src, planned.src, strict=True):
        assert before.src[0].op == after.src[0].op
        for before_buffer, after_buffer in zip(before.src[1:], after.src[1:], strict=True):
            original_table.view(before_buffer.buffer)
            planned_table.view(after_buffer.buffer)
    assert original_table.views == planned_table.views
    assert [buffer.nbytes for buffer in original_table.bases] == [buffer.nbytes for buffer in planned_table.bases]


def test_offline_schedule_preserves_source_view_when_storage_is_contiguous(monkeypatch: pytest.MonkeyPatch) -> None:
    # Given the same contiguous row slice on CPU and QCOM descriptor storage.
    source = Tensor.empty(8, 8, device="CPU").shrink(((1, 3), (0, 8)))
    contiguous = source.contiguous().uop
    expected = callify.contiguous_mops_to_view(contiguous, source.uop)
    qcom = Tensor.empty(8, 8, device="QCOM").shrink(((1, 3), (0, 8))).contiguous()
    matcher = callify.pm_early_transform_tensor_graph
    original_open = Device._Device__get_canonicalized_item

    def forbid_gpu_open(name: str) -> Compiled:
        if name.startswith("QCOM"):
            pytest.fail("offline view scheduling attempted to open a GPU device")
        return original_open(name)

    monkeypatch.setattr(Device, "_Device__get_canonicalized_item", forbid_gpu_open)

    # When the source movement and schedule rules run with static allocator capabilities.
    linear = schedule_outputs((qcom,))

    # Then source slice offset/length survive and the matcher is restored.
    assert expected is not None and expected.src[0].base.op is Ops.SLICE
    actual = contiguous_mops_to_view(qcom.uop, qcom.uop.src[0])
    assert actual is not None and actual.src[0].base.op is Ops.SLICE
    assert actual.src[0].base.arg == expected.src[0].base.arg
    assert actual.src[0].base.src[1].arg == expected.src[0].base.src[1].arg
    assert not linear.src
    assert callify.pm_early_transform_tensor_graph is matcher


def test_export_preserves_real_initialized_weights_when_model_is_lazy(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    # Given real nonzero initializer bytes and an unexecuted original Tensor graph.
    values = (1.25, -2.5, 3.75, 0.125) * 4
    data = struct.pack("<16f", *values)
    x = Tensor.empty(16, dtype=dtypes.float32, device="QCOM")
    weight = Tensor.empty(16, dtype=dtypes.float32, device="QCOM")
    output = x + weight
    model = OfflineModel(Bindings({"input": x}, {"output": output}), (InitializedBuffer(weight.uop.base, data),))
    command = json.loads(os.environ["QCOM_COMPILER_COMMAND"])
    assert isinstance(command, list) and all(isinstance(value, str) for value in command)
    compiler = OfflineQcomCompiler(CompilerCommand(tuple(command), 60))
    original_open = Device._Device__get_canonicalized_item

    def forbid_gpu_open(name: str) -> Compiled:
        if name.startswith("QCOM"):
            pytest.fail("offline compilation attempted to open a GPU device")
        return original_open(name)

    monkeypatch.setattr(Device, "_Device__get_canonicalized_item", forbid_gpu_open)

    # When the model is lowered and genuinely compiled without GPU execution.
    destination = tmp_path / "bundle"
    export_model(model, compiler, destination)

    # Then the exported weight ranges and three-buffer kernel ABI retain the model contract.
    graph = json.loads((destination / "graph.json").read_text())
    weights = (destination / "weights.bin").read_bytes()
    initialized = [allocation for allocation in graph["allocations"] if allocation["weight_offset"] is not None]
    assert len(initialized) == 1
    assert weights == data
    assert graph["backend"] == "qcom-cl" and graph["arch"] == "a630"
    assert len(graph["kernels"]) == 1
    assert len(graph["kernels"][0]["arguments"]) == 3
    assert graph["calls"][0]["global"] == [4, 1, 1]
    assert all(argument[0] == {"kind": "image", "width": 4, "height": 1, "pitch": 64, "element_bytes": 4}
               for argument in graph["kernels"][0]["arguments"])
    assert (destination / "kernel-0.bin").stat().st_size > 256


def test_compiled_original_ir_produces_real_values_when_executed_by_cpu_interpreter(tmp_path: Path) -> None:
    # Given genuine initialized weights and the original lazy arithmetic graph.
    values = (1.25, -2.5, 3.75, 0.125) * 4
    data = struct.pack("<16f", *values)
    x = Tensor.empty(16, dtype=dtypes.float32, device="QCOM")
    weight = Tensor.empty(16, dtype=dtypes.float32, device="QCOM")
    model = OfflineModel(Bindings({"input": x}, {"output": x + weight}), (InitializedBuffer(weight.uop.base, data),))
    command = json.loads(os.environ["QCOM_COMPILER_COMMAND"])
    assert isinstance(command, list) and all(isinstance(value, str) for value in command)
    compiled = export_model(model, OfflineQcomCompiler(CompilerCommand(tuple(command), 60)), tmp_path / "bundle")
    output = memoryview(bytearray(64))
    source = memoryview(bytearray(struct.pack("<16f", *range(16))))
    initialized = memoryview(bytearray(data))

    # When the exact emitted low-level IR executes in the original software interpreter.
    program = compiled.programs[0]
    interpreter = PythonRenderer(Target(device="PYTHON"))
    original_ir = program.src[2]
    workitems = tuple(value for value in original_ir.src if value.op is Ops.SPECIAL)
    assert tuple(value.arg for value in workitems) == ("idx0",)
    assert compiled.manifest.calls[0].local == (1, 1, 1)
    executable_ir = original_ir.substitute({value: value.replace(arg="g0") for value in workitems}, walk=True)
    ir = interpreter.compiler.compile(interpreter.render(list(executable_ir.src)))
    PythonProgram(program.arg.function_name, ir)(output, source, initialized, global_size=(4, 1, 1), local_size=(1, 1, 1))

    # Then its numeric output equals independently calculated values, without a GPU inference claim.
    assert struct.unpack("<16f", output) == tuple(index + value for index, value in enumerate(values))
