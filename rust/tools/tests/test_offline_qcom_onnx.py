from __future__ import annotations

import errno
import hashlib
import os
import struct
from pathlib import Path

import onnx
import pytest
from onnx import TensorProto, helper, numpy_helper
from tinygrad import Context, Tensor, dtypes
from tinygrad.nn.onnx import OnnxPBParser

from model_export import ExportError
from model_export.offline_onnx import load_model


def static_model(path: Path) -> str:
    weight = helper.make_tensor("weight", TensorProto.FLOAT, [4, 4], [1.25, -2.5, 3.75, 0.125] * 4)
    shape = helper.make_tensor("shape", TensorProto.INT64, [2], [4, 4])
    nodes = [helper.make_node("Reshape", ["input", "shape"], ["matrix"]), helper.make_node("Add", ["matrix", "weight"], ["output"])]
    graph = helper.make_graph(nodes, "real_initialized", [helper.make_tensor_value_info("input", TensorProto.FLOAT, [16])],
                              [helper.make_tensor_value_info("output", TensorProto.FLOAT, [4, 4])], [weight, shape])
    onnx.save(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)]), path)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def test_original_onnx_loader_retains_weights_when_control_values_are_static(tmp_path: Path) -> None:
    # Given an actual ONNX model with real weights and a fixed reshape control.
    path = tmp_path / "model.onnx"
    digest = static_model(path)

    # When the original ONNX operators build the offline QCOM graph.
    model = load_model(path, digest)

    # Then original types/shapes and exact initializer bytes survive.
    assert model.bindings.inputs["input"].shape == (16,)
    assert model.bindings.outputs["output"].shape == (4, 4)
    assert all(tensor.device == "QCOM" for tensor in model.bindings.inputs.values())
    assert struct.pack("<16f", *((1.25, -2.5, 3.75, 0.125) * 4)) in tuple(item.data for item in model.initialized)


def test_llvm_rejects_the_original_byte_to_boolean_bitcast() -> None:
    with Context(DEV="CPU:LLVM"):
        with pytest.raises(RuntimeError, match="invalid cast opcode"):
            Tensor(bytes([0, 1, 1, 0]), device="CPU").bitcast(dtypes.bool).data()


@pytest.mark.parametrize("raw", [True, False])
@pytest.mark.parametrize("shape,values", [((), [True]), ((), [False]), ((4,), [True, False, True, False])])
def test_boolean_initializers_keep_exact_bytes_and_shape(tmp_path: Path, raw: bool, shape, values) -> None:
    condition = helper.make_tensor("condition", TensorProto.BOOL, shape, bytes(values) if raw else values, raw=raw)
    zero = helper.make_tensor("zero", TensorProto.FLOAT, [4], [0.0] * 4)
    graph = helper.make_graph([helper.make_node("Where", ["condition", "input", "zero"], ["output"])], "boolean_constant",
                              [helper.make_tensor_value_info("input", TensorProto.FLOAT, [4])],
                              [helper.make_tensor_value_info("output", TensorProto.FLOAT, [4])], [condition, zero])
    path = tmp_path / "boolean.onnx"
    path.write_bytes(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)]).SerializeToString())
    before = path.read_bytes()
    path.chmod(0o444)
    model = load_model(path, hashlib.sha256(before).hexdigest())
    actual, = [item for item in model.initialized if item.buffer.dtype == dtypes.bool]
    assert actual.data == numpy_helper.to_array(condition).tobytes() == bytes(values)
    assert model.bindings.inputs["input"].shape == model.bindings.outputs["output"].shape == (4,)
    assert path.read_bytes() == before and path.stat().st_mode & 0o777 == 0o444


def test_loader_reads_authenticated_model_without_write_access(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    path = tmp_path / "immutable.onnx"
    digest = static_model(path)
    before = path.read_bytes()
    path.chmod(0o444)
    original_open = os.open
    write_attempts = []

    def readonly_open(filename, flags, *args, **kwargs):
        if os.fsdecode(filename) == str(path) and flags & os.O_ACCMODE != os.O_RDONLY:
            write_attempts.append(flags)
            raise PermissionError(errno.EACCES, "immutable model cache", str(path))
        return original_open(filename, flags, *args, **kwargs)

    monkeypatch.setattr(os, "open", readonly_open)
    # Reproduce the real DISK-parser failure even when this test runs as root.
    with pytest.raises(PermissionError, match="immutable model cache"):
        OnnxPBParser(path, load_external_data=False).parse()
    denied = len(write_attempts)
    assert denied > 0

    model = load_model(path, digest)
    assert len(write_attempts) == denied
    assert path.read_bytes() == before
    assert path.stat().st_mode & 0o777 == 0o444
    assert set(tmp_path.iterdir()) == {path}
    assert model.bindings.inputs["input"].shape == (16,)
    assert model.bindings.outputs["output"].shape == (4, 4)
    assert struct.pack("<16f", *((1.25, -2.5, 3.75, 0.125) * 4)) in tuple(item.data for item in model.initialized)


def test_readonly_loader_still_rejects_unbound_external_tensors(tmp_path: Path) -> None:
    path = tmp_path / "external.onnx"
    weight = TensorProto(name="weight", data_type=TensorProto.FLOAT, dims=[16], data_location=TensorProto.EXTERNAL)
    weight.external_data.add(key="location", value="unbound.weights")
    graph = helper.make_graph([helper.make_node("Add", ["input", "weight"], ["output"])], "external",
                              [helper.make_tensor_value_info("input", TensorProto.FLOAT, [16])],
                              [helper.make_tensor_value_info("output", TensorProto.FLOAT, [16])], [weight])
    path.write_bytes(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)]).SerializeToString())
    path.chmod(0o444)
    with pytest.raises(ExportError, match="self-contained|external"):
        load_model(path, hashlib.sha256(path.read_bytes()).hexdigest())
    assert set(tmp_path.iterdir()) == {path}


def test_original_onnx_loader_rejects_file_when_pinned_identity_differs(tmp_path: Path) -> None:
    # Given a genuine model whose requested identity does not match its bytes.
    path = tmp_path / "model.onnx"
    static_model(path)

    # When admission uses an unrelated model identity.
    with pytest.raises(ExportError, match="identity"):
        load_model(path, "0" * 64)


def test_original_onnx_loader_rejects_runtime_content_when_used_as_static_shape(tmp_path: Path) -> None:
    # Given a reshape controlled by runtime data rather than fixed metadata.
    path = tmp_path / "runtime-shape.onnx"
    graph = helper.make_graph([helper.make_node("Reshape", ["input", "shape"], ["output"])], "dynamic_content",
                              [helper.make_tensor_value_info("input", TensorProto.FLOAT, [16]),
                               helper.make_tensor_value_info("shape", TensorProto.INT64, [2])],
                              [helper.make_tensor_value_info("output", TensorProto.FLOAT, [4, 4])])
    onnx.save(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)]), path)

    # When the offline producer needs a compile-time control value.
    with pytest.raises(ExportError, match="runtime content"):
        load_model(path, hashlib.sha256(path.read_bytes()).hexdigest())
