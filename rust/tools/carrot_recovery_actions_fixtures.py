# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run: imported by carrot_recovery_actions_control.py.
from __future__ import annotations

from contextlib import contextmanager
import fcntl
import hashlib
import http.client
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
from collections.abc import Iterator
from types import ModuleType

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "openpilot/selfdrive/carrot/recovery/server.py"
type JsonValue = None | bool | int | float | str | list[JsonValue] | dict[str, JsonValue]
type JsonObject = dict[str, JsonValue]


def git(root: Path, *args: str) -> str:
  result = subprocess.run(["git", *args], cwd=root, capture_output=True, encoding="utf-8", errors="replace", check=True)
  return result.stdout.strip()


def source() -> ModuleType:
  spec = importlib.util.spec_from_file_location("owned_recovery_actions_source", SOURCE)
  assert spec and spec.loader
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


def remote(root: Path) -> Path:
  seed = root / "seed"
  seed.mkdir()
  git(seed, "init", "-b", "main")
  git(seed, "config", "user.name", "Recovery Fixture")
  git(seed, "config", "user.email", "recovery-fixture@example.invalid")
  (seed / "tracked").write_text("initial\n")
  git(seed, "add", "tracked")
  git(seed, "commit", "-m", "initial recovery fixture 복구")
  git(seed, "branch", "topic")
  bare = root / "origin.git"
  git(root, "clone", "--bare", str(seed), str(bare))
  return bare


def clone(root: Path, bare: Path) -> Path:
  git(root.parent, "clone", str(bare), str(root))
  git(root, "config", "user.name", "Recovery Fixture")
  git(root, "config", "user.email", "recovery-fixture@example.invalid")
  common = root / "openpilot/common"
  common.mkdir(parents=True)
  shutil.copyfile(ROOT / "openpilot/common/repo_update.py", common / "repo_update.py")
  return root


def snapshot(repo: Path) -> dict[str, str | list[str]]:
  return {
    "head": git(repo, "rev-parse", "HEAD"),
    "branch": git(repo, "branch", "--show-current"),
    "status": git(repo, "status", "--short", "--untracked-files=all"),
    "refs": git(repo, "for-each-ref", "--format=%(refname) %(objectname) %(upstream)"),
    "fetch": git(repo, "config", "--get-all", "remote.origin.fetch").splitlines(),
    "remote": git(repo, "config", "--get", "remote.origin.url"),
    "tracked": (repo / "tracked").read_text(),
  }


@contextmanager
def environment(values: dict[str, str]) -> Iterator[None]:
  previous = {key: os.environ.get(key) for key in values}
  os.environ.update(values)
  try:
    yield
  finally:
    for key, value in previous.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value


def request(port: int, payload: dict[str, str | int | bool]) -> JsonObject:
  connection = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
  try:
    connection.request("POST", "/api/action", json.dumps(payload), {"Content-Type": "application/json"})
    response = connection.getresponse()
    body = response.read()
    assert response.status == 200, (response.status, body)
    result = json.loads(body)
    assert isinstance(result, dict), body
    return result
  finally:
    connection.close()


@contextmanager
def native(binary: Path, repo: Path, tmux_log: Path, output: Path) -> Iterator[int]:
  with socket.socket() as reserved:
    reserved.bind(("127.0.0.1", 0))
    port = reserved.getsockname()[1]
  launcher = ROOT / "rust/target/debug/openpilot-process-child"
  sibling = binary.with_name("openpilot-process-child")
  if not sibling.exists():
    sibling.symlink_to(launcher)
  command = [str(binary), str(repo), str(tmux_log), str(launcher), str(port)]
  (output / "invocation.json").write_text(json.dumps({"argv": command, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}, indent=2) + "\n")
  with (output / "process.log").open("wb") as log:
    process = subprocess.Popen(command, stdout=log, stderr=log)
    try:
      deadline = time.monotonic() + 10
      while True:
        try:
          with socket.create_connection(("127.0.0.1", port), timeout=0.2):
            break
        except OSError:
          assert process.poll() is None, (output / "process.log").read_text()
          assert time.monotonic() < deadline, "native startup timeout"
          time.sleep(0.01)
      yield port
    finally:
      process.terminate()
      try:
        process.wait(timeout=5)
      except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
      (output / "cleanup.json").write_text(json.dumps({"pid": process.pid, "exit": process.returncode, "reaped": True}) + "\n")


def execute(command: str, repo: Path, output: Path) -> int:
  result = subprocess.run(["bash", "-c", command], cwd=repo, capture_output=True, encoding="utf-8", errors="replace", timeout=30, check=False)
  (output / "shell.json").write_text(json.dumps({"argv": ["bash", "-c", command], "cwd": str(repo), "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}, indent=2) + "\n")
  return result.returncode


def wait_git_quiet(output: Path) -> None:
  spec = importlib.util.spec_from_file_location("owned_repo_update_source", ROOT / "openpilot/common/repo_update.py")
  assert spec and spec.loader
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  deadline = time.monotonic() + 10
  observations = []
  while module.git_process_running():
    active = []
    for entry in Path("/proc").iterdir():
      if not entry.name.isdigit():
        continue
      try:
        name = (entry / "comm").read_text().strip()
      except FileNotFoundError:
        continue
      if name == "git" or name.startswith("git-"):
        active.append({"pid": entry.name, "comm": name})
    observations.append(active)
    emit(output, "git-quiet-precondition.json", {"observed_active_git": observations, "quiet": False})
    assert time.monotonic() < deadline, "source process-inspection precondition could not be established"
    time.sleep(0.01)
  emit(output, "git-quiet-precondition.json", {"observed_active_git": observations, "quiet": True})


@contextmanager
def held_lock(path: Path) -> Iterator[None]:
  with path.open("w") as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    yield


def emit(output: Path, name: str, value: JsonObject | list[JsonValue]) -> None:
  (output / name).write_text(json.dumps(value, ensure_ascii=True, indent=2) + "\n")
