from __future__ import annotations

import ast
from dataclasses import dataclass, field
from pathlib import Path
import threading
import time
from types import SimpleNamespace

from pytest import MonkeyPatch

from usbgpu_boot_fixture import Json


FLAGS = ("UsbGpuPresent", "UsbGpuCompiled", "UsbGpuHardwareSeen", "UsbGpuLoading", "UsbGpuActive", "UsbGpuStartupFailed")


@dataclass(frozen=True, slots=True)
class Parameters:
  values: dict[str, bool] = field(default_factory=dict)

  def get_bool(self, name: str) -> bool:
    return self.values.get(name, False)

  def put_bool(self, name: str, value: bool) -> None:
    self.values[name] = value

  def flags(self) -> dict[str, Json]:
    return {name: self.get_bool(name) for name in FLAGS}


class Log:
  def warning(self, _text: str) -> None:
    pass

  def error(self, _text: str) -> None:
    pass

  def exception(self, _text: str) -> None:
    pass


class OwnedLoaderFailure(RuntimeError):
  pass


def present(devices: Path) -> None:
  port = devices / "fixture"
  port.mkdir(parents=True, exist_ok=True)
  for name, value in {"idVendor": "add1", "idProduct": "0001", "speed": "5000.0"}.items():
    (port / name).write_text(value)


def statements(source: Path) -> tuple[list[ast.stmt], list[ast.stmt], list[ast.stmt], list[ast.stmt]]:
  module = ast.parse(source.read_text())
  main = next(node for node in module.body if isinstance(node, ast.FunctionDef) and node.name == "main")
  prefix_start = next(i for i, node in enumerate(main.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "params")
  prefix_end = next(i for i, node in enumerate(main.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "use_wide_camera")
  loading_start = next(i for i, node in enumerate(main.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "st")
  loading_end = next(i for i, node in enumerate(main.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "jetlink")
  failure = next(
    node
    for node in ast.walk(main)
    if isinstance(node, ast.ExceptHandler) and any(isinstance(child, ast.Assign) and ast.unparse(child.targets[0]) == "model" for child in node.body)
  )
  published = next(node for node in ast.walk(main) if isinstance(node, ast.If) and ast.unparse(node.test) == "usbgpu_startup_pending")
  return main.body[prefix_start:prefix_end], main.body[loading_start:loading_end], failure.body, published.body


def instrument(source_path: Path) -> ast.Module:
  prefix, loading, failure, published = statements(source_path)
  wrapper = ast.parse("def owned_startup():\n  pass").body[0]
  assert isinstance(wrapper, ast.FunctionDef)
  observation = ast.parse(
    """initial = params.flags()
selected = params.get_bool('UsbGpuActive')
if selected:
  if SCENARIO == 'disconnect':
    pass
return {'initial': initial, 'final': params.flags(), 'selected': selected, 'attempts': len(ATTEMPTS)}"""
  ).body
  selected = observation[2]
  assert isinstance(selected, ast.If)
  disconnected = selected.body[0]
  assert isinstance(disconnected, ast.If)
  disconnected.body = failure
  selected.body = published + [disconnected]
  wrapper.body = prefix + loading + observation
  return ast.fix_missing_locations(ast.Module(body=[wrapper], type_ignores=[]))


def source(root: Path, scenario: str, retry_error: str | None) -> dict[str, Json]:
  from openpilot.selfdrive.modeld import big_model, helpers, precompiled_model, precompiled_runner

  params = Parameters()
  if scenario == "startup-failed":
    params.put_bool("UsbGpuStartupFailed", True)
  if scenario == "grace":
    params.put_bool("UsbGpuHardwareSeen", True)
    threading.Thread(target=lambda: (time.sleep(0.3), present(root / "sysfs"))).start()
  attempts = []

  def construct(_width: int, _height: int, _path: Path) -> SimpleNamespace:
    attempts.append(True)
    if scenario == "load-error":
      raise OwnedLoaderFailure("owned loader failure")
    if retry_error is not None and len(attempts) == 1:
      raise OwnedLoaderFailure(retry_error)
    return SimpleNamespace(usbgpu=True, run=lambda *_args: {})

  source_path = Path(__file__).resolve().parents[2] / "openpilot/selfdrive/modeld/modeld.py"
  namespace = {
    "Params": lambda: params,
    "cloudlog": Log(),
    "time": time,
    "threading": threading,
    "usbgpu_compiled_path": helpers.usbgpu_compiled_path,
    "usbgpu_present": helpers.usbgpu_present,
    "wait_for_usbgpu_present": helpers.wait_for_usbgpu_present,
    "usbgpu_pcie_not_ready": helpers.usbgpu_pcie_not_ready,
    "refresh_usbgpu_device_cache": lambda: None,
    "queue_usbgpu_error_tmux": lambda *_args: None,
    "ModelState": lambda *_args: SimpleNamespace(usbgpu=False, run=lambda *_args: {}),
    "vipc_client_main": SimpleNamespace(width=1344, height=760),
    "USBGPU_MODEL_LOAD_TIMEOUT": 120,
    "USBGPU_DISCOVERY_GRACE_SECONDS": 5.0,
    "USBGPU_DISCOVERY_POLL_INTERVAL": 0.1,
    "USBGPU_INIT_ATTEMPTS": 6,
    "USBGPU_INIT_RETRY_INTERVAL": 2.0,
    "buf_main": None,
    "buf_extra": None,
    "model_transform_main": None,
    "model_transform_extra": None,
    "inputs": {},
    "bufs": {},
    "transforms": {},
    "prepare_only": False,
    "SEND_RAW_PRED": False,
    "SCENARIO": scenario,
    "ATTEMPTS": attempts,
  }
  with MonkeyPatch.context() as monkey:
    monkey.setattr(big_model, "model_cache_dir", lambda: root / "cache")
    monkey.setattr(precompiled_model, "model_cache_dir", lambda: root / "cache")
    monkey.setattr(helpers, "Path", lambda name: root / "sysfs" if name == "/sys/bus/usb/devices" else Path(name))
    monkey.setattr(precompiled_runner, "PrecompiledModelState", construct)
    exec(compile(instrument(source_path), str(source_path), "exec"), namespace)
    observe = namespace["owned_startup"]
    assert callable(observe)
    result = observe()
    assert isinstance(result, dict)
    return result


def exception_controls() -> list[dict[str, Json]]:
  from openpilot.selfdrive.modeld.helpers import usbgpu_pcie_not_ready

  cause = RuntimeError("outer loader")
  cause.__cause__ = RuntimeError("PCIe power on failed")
  context = RuntimeError("outer loader")
  context.__context__ = RuntimeError("USB bridge reset failed")
  group = ExceptionGroup("backends unavailable", [RuntimeError("KFD absent"), RuntimeError("AMD:0 does not exist")])
  return [
    {"case": name, "transient": usbgpu_pcie_not_ready(error)}
    for name, error in (("cause", cause), ("context", context), ("exception-group", group), ("timeout-fatal", RuntimeError("timed out")))
  ]
