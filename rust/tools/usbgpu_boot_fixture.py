"""Owned installed artifacts, hardware identity and unchanged source boot boundary."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
from typing import TypeAlias

from pytest import MonkeyPatch

Json: TypeAlias = bool | int | float | str | None | list["Json"] | dict[str, "Json"]


@dataclass(frozen=True, slots=True)
class Fixture:
  root: Path
  cache: Path
  installed: Path
  devices: Path
  identity: Path
  worker: Path
  logs: Path


@dataclass(frozen=True, slots=True)
class Spinner:
  def update(self, _text: str) -> None:
    return None


def setup(root: Path, installed: Path, worker: Path, metadata: Path, speed: int = 5000) -> Fixture:
  cache = root / "cache"
  target = cache / "precompiled" / installed.parent.name
  target.mkdir(parents=True)
  marker = json.loads((installed.parent / "installed.json").read_text())
  (target / "installed.json").write_text(json.dumps(marker))
  for name in ("model.pkl", "runtime.tar.gz"):
    os.link(installed.parent / name, target / name)
  runtime = target / marker["runtime_directory"]
  for source in (installed.parent / marker["runtime_directory"]).rglob("*"):
    destination = runtime / source.relative_to(installed.parent / marker["runtime_directory"])
    if source.is_dir():
      destination.mkdir(parents=True, exist_ok=True)
    elif source.is_file():
      destination.parent.mkdir(parents=True, exist_ok=True)
      os.link(source, destination)
  digest, size = marker["pickle"]["sha256"], marker["pickle"]["size"]
  model = {"model_id": "owned", "filename": "model.pkl", "size": size, "sha256": digest, "url": "https://owned.invalid/model.pkl"}
  os.link(installed, cache / f"model-{digest[:16]}.pkl")
  (cache / "state.json").write_text(json.dumps({"active": model, "previous": None}))
  devices = root / "devices"
  port = devices / "owned-usb"
  port.mkdir(parents=True)
  for name, value in {"idVendor": "add1", "idProduct": "0001", "speed": str(speed)}.items():
    (port / name).write_text(value)
  identity = root / "identity"
  for name, value in {
    "sys/firmware/devicetree/base/model": "comma tizi\0",
    "etc/machine-id": "owned-machine",
    "VERSION": "owned-os",
    "proc/sys/kernel/osrelease": "owned-kernel",
  }.items():
    path = identity / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value)
  logs = root / "workers"
  logs.mkdir()
  wrapper = root / "worker"
  wrapper.write_text(
    f"#!/bin/sh\nexec {shlex.quote(str(worker))} {shlex.quote(str(metadata))} \"$2\" \"$3\" \"$4\" {shlex.quote(str(logs))}/\"$3\"x\"$4\".jsonl\n"
  )
  wrapper.chmod(0o700)
  return Fixture(root, cache, target / "model.pkl", devices, identity, wrapper, logs)


def snapshot(fixture: Fixture) -> dict[str, Json]:
  def read(name: str, root: Path) -> Json:
    path = root / name
    return json.loads(path.read_text()) if path.is_file() else None

  failure = read("last_failure.json", fixture.installed.parent)
  status = read("status.json", fixture.cache)
  return {
    "state": status["state"] if status else None,
    "failure_phase": failure["phase"] if failure else None,
    "rejected": (fixture.installed.parent / "rejected").is_file(),
    "receipt": (fixture.installed.parent / "boot_validation.json").is_file(),
    "inputs": {path.stem: [json.loads(line) for line in path.read_text().splitlines()] for path in sorted(fixture.logs.glob("*.jsonl"))},
  }


def source(fixture: Fixture) -> dict[str, Json]:
  from openpilot.selfdrive.modeld import big_model, helpers, precompiled_model, precompiled_validation
  from openpilot.system.manager import build

  run = subprocess.run
  identity_paths = {"/sys/firmware/devicetree/base/model", "/etc/machine-id", "/VERSION"}
  launches = []

  def smoke(command: list[str], **options: Json) -> subprocess.CompletedProcess[str]:
    actual = [sys.executable, "-P", str(Path(__file__).with_name("usbgpu_smoke_source.py")), command[3], "--worker", str(fixture.worker), *command[4:]]
    launches.append(actual)
    return run(actual, **options)

  with MonkeyPatch.context() as monkey:
    monkey.setattr(big_model, "model_cache_dir", lambda: fixture.cache)
    monkey.setattr(precompiled_model, "model_cache_dir", lambda: fixture.cache)
    monkey.setattr(helpers, "Path", lambda name: fixture.devices if name == "/sys/bus/usb/devices" else Path(name))
    monkey.setattr(precompiled_validation, "Path", lambda name: fixture.identity / str(name).lstrip("/") if str(name) in identity_paths else Path(name))
    monkey.setattr(precompiled_validation.platform, "release", lambda: "owned-kernel")
    monkey.setattr(build.subprocess, "run", smoke)
    try:
      prepared = build.build_usbgpu_model(Spinner())
      error = None
    except OSError as failure:
      prepared, error = None, type(failure).__name__
  return {"prepared": prepared, "error": error, "launches": launches, "observable": snapshot(fixture)}


def native(binary: Path, package: Path, fixture: Fixture) -> dict[str, Json]:
  argv = [
    str(binary),
    "--boot",
    "--root",
    str(fixture.root),
    "--assets",
    str(package),
    "--devices",
    str(fixture.devices),
    "--identity-root",
    str(fixture.identity),
    "--worker",
    str(fixture.worker),
  ]
  before = {path.name: path.stat().st_mtime_ns for path in fixture.logs.glob("*.jsonl")}
  process = subprocess.run(argv, env=dict(os.environ, CARROT_BIG_MODEL_DIR=str(fixture.cache)), text=True, capture_output=True, check=False, timeout=40)
  result = {
    "argv": argv,
    "exit": process.returncode,
    "stdout": process.stdout,
    "stderr": process.stderr,
    "prepared": None,
    "error": process.stderr if process.returncode else None,
    "observable": snapshot(fixture),
    "new_worker_inputs": before != {path.name: path.stat().st_mtime_ns for path in fixture.logs.glob("*.jsonl")},
  }
  if process.returncode == 0:
    result["prepared"] = json.loads(process.stdout.splitlines()[-1])["prepared"]
  return result
