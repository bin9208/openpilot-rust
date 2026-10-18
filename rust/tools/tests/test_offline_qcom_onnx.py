from __future__ import annotations

import hashlib
import struct
from pathlib import Path

import onnx
import pytest
from onnx import TensorProto, helper

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


def test_signal_fp32_export_keeps_full_precision_and_materializes_transposed_output(tmp_path: Path) -> None:
    path = tmp_path / "signal-shape.onnx"
    weight = helper.make_tensor("weight", TensorProto.FLOAT, [2, 3], [1.000123] * 6)
    graph = helper.make_graph([
        helper.make_node("Add", ["input", "weight"], ["sum"]),
        helper.make_node("Transpose", ["sum"], ["output"], perm=[1, 0]),
    ], "signal_fp32", [helper.make_tensor_value_info("input", TensorProto.FLOAT, [2, 3])],
       [helper.make_tensor_value_info("output", TensorProto.FLOAT, [3, 2])], [weight])
    onnx.save(helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)]), path)
    model = load_model(path, hashlib.sha256(path.read_bytes()).hexdigest(), float16=0)
    assert struct.pack("<6f", *([1.000123] * 6)) in tuple(item.data for item in model.initialized)
    from model_export.offline_schedule import schedule_outputs
    from model_export.offline_buffers import descriptor
    schedule_outputs(tuple(model.bindings.outputs.values()), float16=0)
    assert descriptor(model.bindings.outputs["output"].uop).nbytes == 24
