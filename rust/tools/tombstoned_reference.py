# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["pyzmq==27.1.0", "sentry-sdk==2.55.0"]
# ///
# How to run: used by check_tombstoned_reference.py with an unchanged compiled Params binding.
"""Original tombstoned/Sentry policy worker; only hardware/paths/clock/SDK seams are isolated."""
from __future__ import annotations

import ast
from contextlib import contextmanager
import datetime
import importlib.util
import json
import os
from pathlib import Path
from queue import Queue
import subprocess
import sys
from threading import Thread
import time
from types import ModuleType, SimpleNamespace

from check_version_reference import load_source

ROOT = Path(__file__).resolve().parents[2]


def module(name, path):
  spec = importlib.util.spec_from_file_location(name, path)
  result = importlib.util.module_from_spec(spec)
  sys.modules[name] = result
  parent, _, field = name.rpartition(".")
  if parent in sys.modules:
    setattr(sys.modules[parent], field, result)
  spec.loader.exec_module(result)
  return result


def install(name, value):
  sys.modules[name] = value
  parent, _, field = name.rpartition(".")
  if parent in sys.modules:
    setattr(sys.modules[parent], field, value)


class StopLoop(BaseException):
  pass


class Worker:
  def __init__(self, binding):
    _, self.version, _ = load_source()
    for name in ["openpilot.common", "openpilot.system"]:
      install(name, sys.modules[name])
    self.calls = []
    self.logs = []
    self.failure = None
    self.params_root = None
    self.base = None
    self.root = None
    self.device = "tici"
    self.pc = False
    self.stamp = "2020-01-02--03-04-05"
    self.enabled = None
    self.thread = None
    self.advance = Queue()
    self.stopped = Queue()
    self.params_binding = None

    owner = self
    class Logger:
      def record(self, level, message, exception=False):
        owner.logs.append({"levelnum": level, "msg": message, "exception": bool(exception), "created": time.time()})
      def error(self, message, **kwargs): self.record(40, message, kwargs.get("exc_info", False))
      def exception(self, message): self.record(40, message, True)
      def warning(self, message): self.record(30, message)
      def info(self, message): self.record(20, message)
      def debug(self, message): self.record(10, message)
    logger = Logger()
    sys.modules["openpilot.common.swaglog"].cloudlog = logger
    self.version.cloudlog = logger
    os.environ["OPENPILOT_PREFIX"] = "fixture"
    self.params_binding = module("openpilot.common.params_pyx", binding)
    params_module = ModuleType("openpilot.common.params")
    params_module.Params = lambda: self.params_binding.Params(str(self.params_root))
    install(params_module.__name__, params_module)
    registration = ModuleType("openpilot.system.athena.registration")
    registration.Params = params_module.Params
    registration.UNREGISTERED_DONGLE_ID = "UnregisteredDevice"
    definition = next(node for node in ast.parse((ROOT / "openpilot/system/athena/registration.py").read_text()).body if isinstance(node, ast.FunctionDef) and node.name == "is_registered_device")
    exec(compile(ast.Module(body=[definition], type_ignores=[]), str(ROOT / "openpilot/system/athena/registration.py"), "exec"), registration.__dict__)
    install(registration.__name__, registration)
    hardware = ModuleType("openpilot.system.hardware")
    hardware.PC = False
    hardware.HARDWARE = SimpleNamespace(get_device_type=lambda: self.device)
    install(hardware.__name__, hardware)
    paths = ModuleType("openpilot.system.hardware.hw")
    paths.Paths = SimpleNamespace(log_root=lambda: str(self.root))
    install(paths.__name__, paths)
    sdk = ModuleType("sentry_sdk")
    sdk.init = self.init_sdk
    sdk.set_user = lambda value: self.call("set_user", value)
    sdk.set_tag = lambda key, value: self.call("set_tag", {"key": key, "value_json": json.dumps(value)})
    sdk.set_extra = lambda key, value: self.call("set_extra", {"key": key, "value_json": json.dumps(value)})
    sdk.capture_message = lambda message: self.call("capture_message", {"message": message})
    sdk.capture_exception = lambda exception, **kwargs: self.call("capture_exception", exception)
    sdk.flush = lambda: self.call("flush", None)
    @contextmanager
    def scope():
      yield sdk
    sdk.configure_scope = scope
    install("sentry_sdk", sdk)
    threading = ModuleType("sentry_sdk.integrations.threading")
    threading.ThreadingIntegration = lambda **kwargs: kwargs
    install(threading.__name__, threading)
    self.sentry = module("openpilot.system.sentry", ROOT / "openpilot/system/sentry.py")
    self.sentry.get_build_metadata = lambda: self.version.get_build_metadata(str(self.base))
    self.sentry.get_version = lambda: self.version.get_version(str(self.base))
    self.tomb = module("openpilot.system.tombstoned", ROOT / "openpilot/system/tombstoned.py")
    self.tomb.get_build_metadata = self.sentry.get_build_metadata
    self.tomb.datetime = SimpleNamespace(datetime=SimpleNamespace(now=lambda: datetime.datetime.strptime(self.stamp, "%Y-%m-%d--%H-%M-%S")))
    self.tomb.time = SimpleNamespace(sleep=self.sleep)
    self.original_init = self.sentry.init
    def init(project):
      self.enabled = self.original_init(project)
      return self.enabled
    self.sentry.init = init
    report = next(node for node in ast.parse((ROOT / "openpilot/system/tombstoned.py").read_text()).body if isinstance(node, ast.FunctionDef) and node.name == "report_tombstone_apport")
    names = {"clean_path", "new_fn"}
    self.filename_nodes = [node for node in report.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in names for target in node.targets)]

  def call(self, operation, fields):
    path = self.params_root / "fixture/CarrotException"
    value = path.read_text() if path.is_file() else None
    self.calls.append({"op": operation, "fields": fields, "carrot_exception": value, "created": time.time()})
    if self.failure == operation:
      raise RuntimeError("fixture failure")

  def init_sdk(self, dsn, **kwargs):
    project = "selfdrive_native" if dsn == self.sentry.SentryProject.SELFDRIVE_NATIVE.value else "selfdrive"
    self.call("init", {"project": project, "release": kwargs["release"], "environment": kwargs["environment"], "default_integrations": kwargs["default_integrations"], "threading_requested": bool(kwargs["integrations"]), "traces_sample_rate": kwargs["traces_sample_rate"], "max_value_length": kwargs["max_value_length"]})

  def sleep(self, seconds):
    assert seconds == 5
    self.stopped.put(None)
    if not self.advance.get(timeout=90):
      raise StopLoop

  def main(self):
    try:
      self.tomb.main()
    except StopLoop:
      return
    except BaseException as error:  # noqa: BROAD_EXCEPT_OK - relay actual source daemon termination to the driver
      self.stopped.put(error)

  def stop(self):
    if self.thread is not None:
      self.advance.put(False)
      self.thread.join(timeout=5)
      assert not self.thread.is_alive()
      self.thread = None
      self.advance = Queue()
      self.stopped = Queue()

  def handle(self, request):
    match request["op"]:
      case "configure":
        self.stop()
        self.base, self.params_root = Path(request["base"]), Path(request["params"])
        self.pc, self.device = request["pc"], request["device"]
        self.sentry.PC = self.pc
        self.failure = None
        return True
      case "init": return self.sentry.init(self.sentry.SentryProject[request["project"].upper()])
      case "fail": self.failure = request["operation"]
      case "capture": self.sentry.capture_exception(request["exception"], exc_info=request.get("log_exception", True))
      case "tag": self.sentry.set_tag(request["key"], json.loads(request["value"]))
      case "tombstone": self.sentry.report_tombstone(request["filename"], request["message"], request["contents"])
      case "safe": return self.tomb.safe_fn(request["text"])
      case "filename":
        build = self.version.build_metadata_from_dict({"openpilot": {"git_commit": json.loads(request["commit"])}})
        namespace = {"path": request["path"], "date": request["stamp"], "build_metadata": build, "safe_fn": self.tomb.safe_fn, "MAX_TOMBSTONE_FN_LEN": self.tomb.MAX_TOMBSTONE_FN_LEN}
        exec(compile(ast.Module(body=self.filename_nodes, type_ignores=[]), str(ROOT / "openpilot/system/tombstoned.py"), "exec"), namespace)
        return namespace["new_fn"]
      case "scan":
        self.tomb.APPORT_DIR = request["path"] + "/"
        return sorted(self.tomb.get_tombstones())
      case "clear":
        self.tomb.APPORT_DIR = request["path"] + "/"
        self.tomb.clear_apport_folder()
      case "retrace" | "report":
        def checked(command, **kwargs):
          assert kwargs["timeout"] == 30 and kwargs["shell"] and kwargs["encoding"] == "utf8"
          kwargs["timeout"] = request.get("timeout_ms", 30_000) / 1000
          kwargs["executable"] = request.get("shell", "/bin/bash")
          return subprocess.check_output(command, **kwargs)
        self.tomb.subprocess = SimpleNamespace(check_output=checked, CalledProcessError=subprocess.CalledProcessError, TimeoutExpired=subprocess.TimeoutExpired)
        if request["op"] == "retrace": return self.tomb.get_apport_stacktrace(request["path"])
        self.root, self.stamp = Path(request["root"]), request["stamp"]
        self.tomb.report_tombstone_apport(request["path"])
      case "start":
        self.tomb.subprocess = subprocess
        self.tomb.APPORT_DIR = request["apport"] + "/"
        self.root, self.stamp = Path(request["root"]), request["stamp"]
        self.thread = Thread(target=self.main)
        self.thread.start()
        error = self.stopped.get(timeout=40)
        if error is not None: raise error
        return self.enabled
      case "cycle":
        self.advance.put(True)
        error = self.stopped.get(timeout=40)
        if error is not None: raise error
      case _: raise AssertionError(request)
    return None


def main():
  worker = Worker(Path(sys.argv[1]))
  try:
    for line in sys.stdin:
      worker.calls.clear()
      worker.logs.clear()
      try:
        result = {"value": worker.handle(json.loads(line))}
      except Exception as error:  # noqa: BROAD_EXCEPT_OK - oracle reports the unchanged source exception class
        result = {"error": type(error).__name__}
      print(json.dumps({"result": result, "calls": worker.calls, "logs": worker.logs}), flush=True)
  finally:
    worker.stop()


if __name__ == "__main__":
  main()
