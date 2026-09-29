from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import numpy as np
import pytest
from tinygrad import Tensor, TinyJit

from model_export import Bindings, ExportError, export_cpu


def test_recurrent_capture_runs_in_rust_with_aliased_outputs(tmp_path: Path) -> None:
    state = Tensor(np.arange(64, dtype=np.float32)).realize()
    inputs = Tensor(np.ones(64, dtype=np.float32)).realize()

    @TinyJit
    def recurrent(x: Tensor) -> Tensor:
        return state.assign(state * 0.5 + x).realize()

    recurrent(inputs)
    output = recurrent(inputs)
    export_cpu(recurrent, Bindings(inputs={"x": inputs}, outputs={"tail": output[8:24], "all": output}), tmp_path / "bundle")
    sequence = []
    expected = []
    for index in range(4):
        values = np.linspace(-index, index + 1, 64, dtype=np.float32)
        inputs.assign(Tensor(values)).realize()
        expected.append(recurrent(inputs).numpy().copy())
        (tmp_path / f"input-{index}.bin").write_bytes(values.tobytes())
        sequence.append({"inputs": {"x": f"input-{index}.bin"}, "outputs": {"all": f"all-{index}.bin", "tail": f"tail-{index}.bin"}})
    (tmp_path / "sequence.json").write_text(json.dumps(sequence))
    binary = Path(os.environ["MODEL_RUN_BINARY"])
    subprocess.run([binary, "--trusted-bundle", tmp_path / "bundle", tmp_path / "sequence.json"], check=True)
    for index, reference in enumerate(expected):
        np.testing.assert_array_equal(np.fromfile(tmp_path / f"all-{index}.bin", dtype=np.float32), reference)
        np.testing.assert_array_equal(np.fromfile(tmp_path / f"tail-{index}.bin", dtype=np.float32), reference[8:24])


def test_rejects_uncaptured_jit_without_emitting_bundle(tmp_path: Path) -> None:
    inputs = Tensor([1.0]).realize()
    jit = TinyJit(lambda x: (x + 1).realize())
    with pytest.raises(ExportError, match="capture"):
        export_cpu(jit, Bindings(inputs={"x": inputs}, outputs={"x": inputs}), tmp_path / "bundle")
    assert not (tmp_path / "bundle" / "graph.json").exists()


def test_static_positional_arguments_do_not_shift_tensor_bindings(tmp_path: Path) -> None:
    inputs = Tensor(np.arange(64, dtype=np.float32)).realize()
    bias = Tensor(np.full(64, 5.0, dtype=np.float32)).realize()
    jit = TinyJit(lambda enabled, x, *, bias: (x * 2 + bias if enabled else x).realize())
    jit(True, inputs, bias=bias)
    output = jit(True, inputs, bias=bias)
    export_cpu(jit, Bindings({"bias": bias, "input": inputs}, {"output": output}), tmp_path / "bundle")
    values = np.arange(64, dtype=np.float32) + 7
    (tmp_path / "input.bin").write_bytes(values.tobytes())
    (tmp_path / "sequence.json").write_text(json.dumps([{"inputs": {"input": "input.bin"}, "outputs": {"output": "output.bin"}}]))
    subprocess.run([Path(os.environ["MODEL_RUN_BINARY"]), "--trusted-bundle", tmp_path / "bundle", tmp_path / "sequence.json"], check=True)
    np.testing.assert_array_equal(np.fromfile(tmp_path / "output.bin", dtype=np.float32), values * 2 + 5)


def test_missing_keyword_binding_is_a_contract_error(tmp_path: Path) -> None:
    inputs = Tensor(np.arange(64, dtype=np.float32)).realize()
    jit = TinyJit(lambda *, value: (value * 2).realize())
    jit(value=inputs)
    output = jit(value=inputs)
    with pytest.raises(ExportError, match="binding names.*input contract"):
        export_cpu(jit, Bindings({"wrong": inputs}, {"output": output}), tmp_path / "bundle")
    assert not (tmp_path / "bundle").exists()


def test_rejects_bindings_that_disagree_with_the_captured_input_contract(tmp_path: Path) -> None:
    inputs = Tensor(np.ones(64, dtype=np.float32)).realize()
    jit = TinyJit(lambda x: (x * 2).realize())
    jit(inputs)
    output = jit(inputs)
    wrong_size = Tensor(np.ones(4, dtype=np.float32)).realize()
    with pytest.raises(ExportError, match="input contract"):
        export_cpu(jit, Bindings({"input": wrong_size}, {"output": output}), tmp_path / "bundle")


def test_preserves_host_to_cpu_copy_and_keyword_slot_order(tmp_path: Path) -> None:
    left = Tensor(np.arange(64, dtype=np.float32), device="NPY").realize()
    right = Tensor(np.full(64, 3.0, dtype=np.float32)).realize()

    @TinyJit
    def subtract(*, left: Tensor, right: Tensor) -> Tensor:
        return (left.to("CPU") - right).realize()

    subtract(left=left, right=right)
    output = subtract(left=left, right=right)
    export_cpu(subtract, Bindings(inputs={"right": right, "left": left}, outputs={"difference": output}), tmp_path / "bundle")
    values = np.arange(64, dtype=np.float32) * 2
    (tmp_path / "left.bin").write_bytes(values.tobytes())
    (tmp_path / "sequence.json").write_text(json.dumps([
        {"inputs": {"left": "left.bin"}, "outputs": {"difference": "difference.bin"}}
    ]))
    subprocess.run([Path(os.environ["MODEL_RUN_BINARY"]), "--trusted-bundle", tmp_path / "bundle", tmp_path / "sequence.json"], check=True)
    np.testing.assert_array_equal(np.fromfile(tmp_path / "difference.bin", dtype=np.float32), values - 3)
