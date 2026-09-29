from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest
from tinygrad import Tensor, TinyJit, dtypes
from tinygrad.device import Buffer, Device
from tinygrad.engine.jit import CapturedJit, _prepare_jit_inputs
from tinygrad.renderer.cstyle import QCOMCLRenderer
from tinygrad.runtime.ops_python import PythonAllocator
from tinygrad.uop.ops import Ops, ProgramInfo, UOp

from model_export import Bindings, ExportError
from model_export.qcom import export_qcom
from model_export.schema import Entrypoint


def captured_qcom(monkeypatch):
    original = Device._Device__get_canonicalized_item
    renderer = QCOMCLRenderer.__new__(QCOMCLRenderer)
    renderer.target = SimpleNamespace(arch="a630")
    fake = SimpleNamespace(allocator=PythonAllocator(None), renderer=renderer, synchronize=lambda: None)
    monkeypatch.setattr(Device, "_Device__get_canonicalized_item", lambda name: fake if name == "QCOM" else original(name))
    buffer = Buffer("QCOM", 8, dtypes.float32, initial_value=bytes(range(32)))
    state = Tensor(UOp.from_buffer(buffer))
    _, _, names, info = _prepare_jit_inputs((state,), {})
    path = Path(__file__).parents[2] / "crates/model-runtime/tests/fixtures/qcom/buffer_add.bin"
    signature = ProgramInfo(name="buffer_add", global_size=(8, 1, 1), local_size=(1, 1, 1), globals=(0, 1), outs=(0,), ins=(1,),
                            aux=((((0, dtypes.float32.ptr(8)),), ((1, dtypes.float32.ptr(8)),)),))
    program = UOp(Ops.PROGRAM, src=(UOp(Ops.SINK), UOp(Ops.DEVICE, arg="QCOM"), UOp(Ops.LINEAR),
                                  UOp(Ops.SOURCE, arg="fixture"), UOp(Ops.BINARY, arg=path.read_bytes())), arg=signature)
    parameter = state.uop.param_like(0)
    jit = TinyJit(lambda value: value)
    jit.captured = CapturedJit(state, UOp(Ops.LINEAR, src=(program.call(parameter, parameter),)), names, info)
    return jit, state


@pytest.mark.parametrize("entries", [(), (Entrypoint("prepare", 0, 0), Entrypoint("model", 0, 1))])
def test_qcom_capture_preserves_kernel_bytes_and_aliased_storage(monkeypatch, tmp_path, entries):
    jit, state = captured_qcom(monkeypatch)
    destination = tmp_path / "bundle"
    export_qcom(jit, Bindings({"state": state}, {"output": state}), destination, entrypoints=entries)
    graph = json.loads((destination / "graph.json").read_text())
    assert graph["backend"] == "qcom-cl"
    assert graph["version"] == (2 if entries else 1)
    assert ("entrypoints" in graph) == bool(entries)
    assert len(graph["allocations"]) == 1
    assert graph["inputs"][0]["view"] == graph["outputs"][0]["view"]
    assert graph["calls"][0]["views"] == [0, 0]
    assert (destination / "weights.bin").read_bytes() == bytes(range(32))
    binary = Path(os.environ["MODEL_RUN_BINARY"]).with_name("qcom-model-run")
    report = subprocess.run([binary, "--check-bundle", destination], check=True, capture_output=True, text=True)
    assert json.loads(report.stdout)["gpu_executed"] is False


def test_qcom_export_rejects_mismatched_inputs(monkeypatch, tmp_path):
    jit, state = captured_qcom(monkeypatch)
    wrong = Tensor(UOp.from_buffer(Buffer("QCOM", 4, dtypes.float32, initial_value=bytes(16))))
    with pytest.raises(ExportError, match="input contract"):
        export_qcom(jit, Bindings({"state": wrong}, {"output": state}), tmp_path / "bundle")
    assert not (tmp_path / "bundle").exists()


def test_qcom_capture_supports_non_tensor_positional_arguments(monkeypatch, tmp_path):
    jit, state = captured_qcom(monkeypatch)
    _, _, names, info = _prepare_jit_inputs((True, state), {})
    jit.captured = CapturedJit(state, jit.captured.linear, names, info)
    export_qcom(jit, Bindings({"state": state}, {"output": state}), tmp_path / "bundle")
    graph = json.loads((tmp_path / "bundle/graph.json").read_text())
    assert graph["calls"][0]["views"] == [0, 0]
