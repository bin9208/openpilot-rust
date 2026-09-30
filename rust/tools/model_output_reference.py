from __future__ import annotations

import ast
import math
from pathlib import Path
from types import ModuleType, SimpleNamespace
from typing import TypeAlias

import numpy as np

from openpilot.cereal import log
from openpilot.selfdrive.modeld.constants import ModelConstants, Plan
from openpilot.selfdrive.modeld.parse_model_outputs import safe_exp, sigmoid

Value: TypeAlias = dict[str, "Value"] | list["Value"] | float | int | bool | str | bytes


def new_message(service: str, valid: bool = False):
    message = log.Event.new_message(valid=valid)
    message.init(service)
    return message


def original_functions() -> tuple[ModuleType, ModuleType]:
    root = Path(__file__).resolve().parents[2]
    action = ModuleType("original_action")
    action.__dict__.update(np=np, log=log, Plan=Plan, ModelConstants=ModelConstants,
                           DT_MDL=0.05, MIN_SPEED=1.0, MIN_LAT_CONTROL_SPEED=0.3, LONG_SMOOTH_SECONDS=0.3)
    sources = {
        "openpilot/selfdrive/controls/lib/drive_helpers.py": {
            "get_accel_from_plan", "get_curvature_from_plan", "curv_from_psis", "smooth_value"},
        "openpilot/selfdrive/modeld/modeld.py": {"get_action_from_model"},
    }
    for path, names in sources.items():
        tree = ast.parse((root / path).read_text())
        nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
        assert {node.name for node in nodes} == names
        exec(compile(ast.Module(body=nodes, type_ignores=[]), path, "exec"), action.__dict__)
    driver = ModuleType("original_driver")
    driver.__dict__.update(sigmoid=sigmoid, safe_exp=safe_exp, messaging=SimpleNamespace(new_message=new_message))
    path = "openpilot/selfdrive/modeld/dmonitoringmodeld.py"
    names = {"parse_model_output", "fill_driver_data", "get_driverstate_packet"}
    nodes = [node for node in ast.parse((root / path).read_text()).body if isinstance(node, ast.FunctionDef) and node.name in names]
    assert {node.name for node in nodes} == names
    exec(compile(ast.Module(body=nodes, type_ignores=[]), path, "exec"), driver.__dict__)
    return action, driver


def compare(expected: Value, actual: Value, path: str = "event") -> int:
    match expected, actual:
        case dict(), dict():
            assert expected.keys() == actual.keys(), (path, expected.keys() ^ actual.keys())
            return sum(compare(value, actual[key], f"{path}.{key}") for key, value in expected.items())
        case list(), list():
            assert len(expected) == len(actual), (path, len(expected), len(actual))
            return sum(compare(left, right, f"{path}[{i}]") for i, (left, right) in enumerate(zip(expected, actual, strict=True)))
        case float(), float():
            tolerance = 2e-5 if "Coefficients" in path else 1e-6
            equal = (math.isnan(expected) and math.isnan(actual)) or expected == actual
            assert equal or math.isclose(expected, actual, rel_tol=tolerance, abs_tol=tolerance), (path, expected, actual)
        case _:
            assert expected == actual, (path, expected, actual)
    return 1
