"""Original calibration estimator and packet oracle with bounded test dependencies."""
from __future__ import annotations

import ast
import logging
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import Optional

import capnp
import numpy as np

from openpilot.cereal import log
from openpilot.common.constants import CV
from openpilot.common.transformations.orientation import euler_from_rot, rot_from_euler

ROOT = Path(__file__).resolve().parents[2]
STATE_ATOL = STATE_RTOL = 2e-12


class Parameters:
    def __init__(self, saved: bytes | None = None):
        self.saved = saved
        self.writes: list[bytes] = []
        self.trim = 0.0

    def get(self, key: str) -> bytes | None:
        assert key == "CalibrationParams"
        return self.saved

    def put_nonblocking(self, key: str, value: bytes) -> None:
        assert key == "CalibrationParams"
        self.writes.append(value)

    def get_float(self, key: str) -> float:
        assert key == "CameraYawTrimDeg"
        return self.trim


def original(mici: bool, params: Parameters) -> ModuleType:
    source = ROOT / "openpilot/cereal/messaging/__init__.py"
    tree = ast.parse(source.read_text(), filename=str(source))
    tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "new_message"]
    scope = {"Optional": Optional, "capnp": capnp, "log": log, "time": SimpleNamespace(monotonic=lambda: 0.0)}
    exec(compile(tree, str(source), "exec"), scope)
    path = ROOT / "openpilot/selfdrive/locationd/calibrationd.py"
    tree = ast.parse(path.read_text(), filename=str(path))
    tree.body = [node for node in tree.body
                 if not (isinstance(node, ast.ImportFrom) and (node.module or "").startswith("openpilot"))
                 and not (isinstance(node, ast.Import) and any(alias.name.startswith("openpilot") for alias in node.names))
                 and not (isinstance(node, ast.If) and isinstance(node.test, ast.Compare)
                          and isinstance(node.test.left, ast.Name) and node.test.left.id == "__name__")]
    module = ModuleType("calibration_source")
    module.__dict__.update(log=log, CV=CV, rot_from_euler=rot_from_euler, euler_from_rot=euler_from_rot,
                           HARDWARE=SimpleNamespace(get_device_type=lambda: "mici" if mici else "pc"),
                           Params=lambda: params, cloudlog=logging.getLogger("calibration-source"),
                           messaging=SimpleNamespace(new_message=scope["new_message"], PubMaster=type("PubMaster", (), {})))
    exec(compile(tree, str(path), "exec"), module.__dict__)
    # Execute the original main-loop body for freeze and publication gating comparisons.
    main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "main")
    body = next(node.body for node in main.body if isinstance(node, ast.While))
    module.loop_body = compile(ast.Module(body=body, type_ignores=[]), str(path), "exec")
    return module


def close(actual: float, expected: float, wire: bool = False) -> bool:
    if np.isnan(expected):
        return bool(np.isnan(actual))
    if np.isinf(expected):
        return actual == expected
    if not np.isfinite(actual):
        return False
    tolerance = STATE_ATOL + STATE_RTOL * abs(expected)
    if wire:
        # Preserve finite Float32 rounding within one adjacent value, with the same
        # 2e-12 cancellation floor used by the Float64 rotation/state oracle.
        with np.errstate(over="ignore"):
            neighbors = [np.nextafter(np.float32(expected), bound, dtype=np.float32)
                         for bound in (np.float32(-np.inf), np.float32(np.inf))]
        tolerance = max([STATE_ATOL, *(abs(float(value) - expected) for value in neighbors if np.isfinite(value))])
    return abs(actual - expected) <= tolerance


def compare_packet(actual: dict, expected: dict) -> int:
    assert actual.keys() == expected.keys(), (actual.keys(), expected.keys())
    fields = 0
    for key, value in expected.items():
        observed = actual[key]
        if isinstance(value, dict):
            fields += compare_packet(observed, value)
        elif isinstance(value, list):
            assert len(observed) == len(value), (key, observed, value)
            assert all(close(a, b, wire=True) for a, b in zip(observed, value, strict=True)), (key, observed, value)
            fields += len(value)
        else:
            assert observed == value, (key, observed, value)
            fields += 1
    return fields


def compare_state(actual: dict, calibrator) -> None:
    for key, source in (("rpy", "rpy"), ("wide", "wide_from_device_euler"), ("spread", "calib_spread"), ("old_rpy", "old_rpy")):
        expected = getattr(calibrator, source).tolist()
        observed = list(map(float, actual[key]))
        assert len(observed) == len(expected), (key, observed, expected)
        assert all(close(a, b) for a, b in zip(observed, expected, strict=True)), (key, observed, expected)
    assert close(float(actual["height"]), float(calibrator.height[0]))
    assert float(actual["old_weight"]) == calibrator.old_rpy_weight
    assert actual["valid_indices"] == calibrator.get_valid_idxs()
    for key in ("idx", "block_idx", "valid_blocks"):
        assert actual[key] == getattr(calibrator, key), (key, actual[key], getattr(calibrator, key))
    assert actual["status"] == ["uncalibrated", "calibrated", "invalid", "recalibrating"][calibrator.cal_status]
