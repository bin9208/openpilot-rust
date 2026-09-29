from __future__ import annotations

import json
import os
import pickle
import subprocess
from pathlib import Path

import numpy as np
import pytest
from tinygrad import Tensor, TinyJit, Context
from tinygrad.uop.ops import Ops

from model_export import Bindings, ExportError, export_cpu
from model_export.pipeline import Stage, compose, restore_inputs


@pytest.mark.parametrize("jit_mode", [1, 2])
def test_imported_stages_share_bridge_and_preserve_prepare_only_state(tmp_path: Path, jit_mode: int) -> None:
    source = Tensor(np.arange(16, dtype=np.float32)).realize()
    state = Tensor(np.zeros(16, dtype=np.float32)).realize()
    prepare = TinyJit(lambda frame: (frame * 2).realize() + 3)
    policy = TinyJit(lambda warped, state: state.assign(state + warped).realize())
    with Context(JIT=jit_mode):
        prepare(source)
        warped = prepare(source)
        policy(warped, state)
        policy(warped, state)
    if jit_mode == 1:
        assert any(call.src[0].op is Ops.CUSTOM_FUNCTION for call in prepare.captured.linear.src)
    prepare, policy = pickle.loads(pickle.dumps((prepare, policy)))
    prepared = restore_inputs(prepare)
    recurrent = restore_inputs(policy, {"0": prepare.captured.ret})
    stages = (Stage("prepare", prepare, prepared), Stage("policy", policy, recurrent))
    bindings = Bindings({"frame": prepared["0"], "state": recurrent["1"]},
                        {"warped": prepare.captured.ret, "state": recurrent["1"]})
    pipeline = compose(stages, bindings)
    export_cpu(pipeline.jit, bindings, tmp_path / "bundle", entrypoints=pipeline.entrypoints)
    graph = json.loads((tmp_path / "bundle/graph.json").read_text())
    bridge = next(binding["view"] for binding in graph["outputs"] if binding["name"] == "warped")
    policy_entry = graph["entrypoints"][1]
    assert any(bridge in call["views"] for call in graph["calls"][policy_entry["start"]:policy_entry["end"]])
    frames = [np.arange(16, dtype=np.float32) + i for i in range(3)]
    sequence = []
    for index, frame in enumerate(frames):
        (tmp_path / f"frame-{index}.bin").write_bytes(frame.tobytes())
        sequence.append({"entrypoint":"prepare", "inputs":{"frame":f"frame-{index}.bin"},
                         "outputs":{"state":f"before-{index}.bin"}})
        if index != 1:
            sequence.append({"entrypoint":"policy", "inputs":{}, "outputs":{"state":f"state-{index}.bin"}})
    (tmp_path / "sequence.json").write_text(json.dumps(sequence))
    subprocess.run([Path(os.environ["MODEL_RUN_BINARY"]), "--trusted-bundle", tmp_path / "bundle", tmp_path / "sequence.json"], check=True)
    np.testing.assert_array_equal(np.fromfile(tmp_path / "before-0.bin", dtype=np.float32), np.zeros(16, dtype=np.float32))
    np.testing.assert_array_equal(np.fromfile(tmp_path / "before-1.bin", dtype=np.float32), frames[0] * 2 + 3)
    np.testing.assert_array_equal(np.fromfile(tmp_path / "before-2.bin", dtype=np.float32), frames[0] * 2 + 3)
    np.testing.assert_array_equal(np.fromfile(tmp_path / "state-2.bin", dtype=np.float32), (frames[0] + frames[2]) * 2 + 6)


def test_restored_input_rejects_incompatible_bridge() -> None:
    source = Tensor(np.arange(16, dtype=np.float32)).realize()
    jit = TinyJit(lambda x: (x + 1).realize())
    jit(source)
    jit(source)
    with pytest.raises(ExportError, match="input contract"):
        restore_inputs(jit, {"0":Tensor(np.zeros(8, dtype=np.float32)).realize()})
    with pytest.raises(ExportError, match="override"):
        restore_inputs(jit, {"unknown":source})
