from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import numpy as np
import pytest
from tinygrad import Tensor, TinyJit

from model_export import Bindings, export_cpu
from openpilot.selfdrive.modeld.compile_dm_warp import make_warp_dm
from openpilot.selfdrive.modeld.compile_modeld import NV12Frame, make_warp
from openpilot.system.camerad.cameras.nv12_info import get_nv12_info


@pytest.mark.parametrize("resolution", [(1928, 1208), (1344, 760)])
@pytest.mark.parametrize("driver_monitoring", [False, True])
def test_original_nv12_warps_match_rust_for_borders_and_projective_transforms(
    tmp_path: Path, resolution: tuple[int, int], driver_monitoring: bool,
) -> None:
    width, height = resolution
    nv12 = NV12Frame(width, height, *get_nv12_info(width, height))
    random = np.random.default_rng(20260930)
    frame = Tensor(random.integers(0, 256, nv12.size, dtype=np.uint8)).realize()
    transform_data = np.eye(3, dtype=np.float32)
    transform = Tensor(transform_data, device="NPY").realize()
    if driver_monitoring:
        jit = TinyJit(make_warp_dm(nv12, 1440, 960))
        inputs = {"input_frame": frame, "M_inv": transform}
    else:
        jit = TinyJit(make_warp(nv12, 512, 256, 4))
        wide_frame = Tensor(random.integers(0, 256, nv12.size, dtype=np.uint8)).realize()
        wide_transform = Tensor(np.eye(3, dtype=np.float32), device="NPY").realize()
        inputs = {"tfm": transform, "big_tfm": wide_transform, "frame": frame, "big_frame": wide_frame}
    jit(**inputs).realize()
    output = jit(**inputs).realize()
    export_cpu(jit, Bindings(inputs, {"warped": output}), tmp_path / "bundle")
    matrices = [np.eye(3, dtype=np.float32),
                np.array([[0.5, 0.01, -40], [-0.01, 0.8, -30], [0.0002, -0.0001, 1]], dtype=np.float32),
                np.array([[1, 0, 0.5], [0, 1, 0.5], [0, 0, 1]], dtype=np.float32)]
    steps = []
    expected = []
    for index, matrix in enumerate(matrices):
        values = random.integers(0, 256, nv12.size, dtype=np.uint8)
        frame.assign(Tensor(values)).realize()
        transform_data[:] = matrix
        expected.append(jit(**inputs).realize().numpy().copy())
        step_inputs = {}
        for number, (name, tensor) in enumerate(inputs.items()):
            path = f"input-{index}-{number}.bin"
            (tmp_path / path).write_bytes(tensor.numpy().tobytes())
            step_inputs[name] = path
        steps.append({"inputs": step_inputs, "outputs": {"warped": f"warped-{index}.bin"}})
    sequence = tmp_path / "sequence.json"
    sequence.write_text(json.dumps(steps))
    subprocess.run([Path(os.environ["MODEL_RUN_BINARY"]), "--trusted-bundle", tmp_path / "bundle", sequence], check=True)
    for index, reference in enumerate(expected):
        actual = np.fromfile(tmp_path / f"warped-{index}.bin", dtype=reference.dtype).reshape(reference.shape)
        np.testing.assert_array_equal(actual, reference)
