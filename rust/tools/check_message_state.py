"""Shared original messaging adapters; run check-message-state for the native CLI."""
from __future__ import annotations

import ast
from pathlib import Path
from types import SimpleNamespace
from typing import Optional, Union

import capnp

from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from openpilot.common.utils import MovingAverage


def source():
    path = Path(__file__).resolve().parents[2] / "openpilot/cereal/messaging/__init__.py"
    tree = ast.parse(path.read_text())
    nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef))
             and node.name in ("new_message", "FrequencyTracker", "SubMaster")]
    environment = {"simulation": "0"}
    scope = {"Optional": Optional, "List": list, "Union": Union, "Dict": dict, "capnp": capnp,
             "log": log, "SERVICE_LIST": SERVICE_LIST, "MovingAverage": MovingAverage, "time": SimpleNamespace(monotonic=lambda: 0.),
             "Poller": object, "sub_sock": lambda *args, **kwargs: None,
             "os": SimpleNamespace(getenv=lambda key, default: environment.get(key.lower(), default))}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), "exec"), scope)
    return scope, environment


def compare(expected, actual, path=""):
    if isinstance(expected, dict):
        assert expected.keys() == actual.keys(), (path, expected.keys(), actual.keys())
        return max((compare(value, actual[key], f"{path}.{key}") for key, value in expected.items()), default=0.)
    if isinstance(expected, list):
        assert len(expected) == len(actual), path
        return max((compare(a, b, f"{path}[{i}]") for i, (a, b) in enumerate(zip(expected, actual, strict=True))), default=0.)
    if isinstance(expected, float):
        error = abs(expected - actual)
        assert error <= 1e-12, (path, expected, actual)
        return error
    assert expected == actual, (path, expected, actual)
    return 0.


if __name__ == "__main__":
    raise SystemExit("This checker moved to the native check-message-state command; use --binary and --output there.")
