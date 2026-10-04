from __future__ import annotations

from dataclasses import dataclass
import gc
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import tempfile
import time
from typing import Literal

from openpilot.cereal import messaging
from plannerd_ipc import OUTPUTS, peers_ready, policy
from plannerd_owner_fixtures import SERVICES, car_params, messages, parameters

ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  phase: Literal["wait", "ready", "recover", "integer", "float", "malformed"]
  signal: int = signal.SIGTERM
  car_params: Literal["missing", "directory", "empty"] = "missing"
  directories: tuple[str, ...] = ()


def no_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def waiting(child: subprocess.Popen[bytes], paths: list[Path]) -> None:
  deadline = time.monotonic() + 15
  while time.monotonic() < deadline:
    assert child.poll() is None, ("exited before Params wait", child.returncode)
    if any("plannerd is waiting for CarParams" in path.read_text() for path in paths):
      time.sleep(.12)
      assert child.poll() is None, ("exited while waiting", child.returncode)
      return
    time.sleep(.01)
  raise TimeoutError("Params wait was not observed")


def frames(publisher, subscribers, child, output: Path, count: int) -> list[str]:
  seen = []
  for index in range(count):
    packets = messages(messaging.new_message, 45 + index, time.monotonic())
    packets["selfdriveState"].selfdriveState.experimentalMode = True
    for name, packet in packets.items():
      if name != "modelV2":
        publisher.send(name, packet)
    publisher.send("modelV2", packets["modelV2"])
    if not subscribers:
      return seen
    deadline = time.monotonic() + 3
    current = set()
    while time.monotonic() < deadline and set(OUTPUTS) != current:
      assert child.poll() is None, ("exited during publication", child.returncode)
      for name, stream in subscribers.items():
        while (raw := stream.receive(non_blocking=True)) is not None:
          path = output / f"publication-{index}-{len(seen)}-{name}.bin"
          path.write_bytes(raw)
          service, _valid, _value = policy(raw)
          assert service == name
          current.add(name)
          seen.append(name)
      time.sleep(.001)
    assert set(OUTPUTS) == current, ("missing publications", current)
    time.sleep(.05)
  return seen


def run(args, case: Case, mode: str):
  output = args.output / case.name / mode
  output.mkdir(parents=True)
  result = {"case": case.name, "mode": mode, "ok": False}
  with tempfile.TemporaryDirectory(prefix="msgq_planner_fault_", dir="/dev/shm") as shared:
    prefix = Path(shared).name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = prefix
    publisher = messaging.PubMaster(SERVICES)
    subscribers = {}
    params = output / "params" / prefix
    params.mkdir(parents=True)
    for key, value in (parameters() | {"EnableRadarTracks": "1", "MyDrivingModeAuto": "0"}).items():
      (params / key).write_text(value)
    for key in case.directories:
      (params / key).unlink(missing_ok=True)
      (params / key).mkdir()
    if case.phase == "integer":
      (params / "EnableRadarTracks").write_text("invalid")
    if case.phase == "float":
      (params / "LatMpcPathCost").write_text("invalid")
    cp = params / "CarParams"
    if case.car_params == "directory":
      cp.mkdir()
    elif case.car_params == "empty":
      cp.write_bytes(b"")
    if case.phase not in ["wait", "recover"]:
      cp.write_bytes(b"x" if case.phase == "malformed" else car_params().to_bytes())
    environment = dict(os.environ, PARAMS_ROOT=str(params.parent), PWD=str(ROOT), LOGPRINT="info",
                       PYTHONDONTWRITEBYTECODE="1", OPENBLAS_NUM_THREADS="1", PYTHONUNBUFFERED="1", SIMULATION="0")
    command = ([str(args.python), "-u", "-P", str(ROOT / "rust/tools/plannerd_daemon_source.py"),
                "--binding", str(args.binding), "--source-native", str(args.source_native), "--log-root", str(output / "logs")]
               if mode == "source" else [str(args.binary), "--solver", str(args.artifact)])
    (output / "invocation.json").write_text(json.dumps({"argv": command, "prefix": prefix}, indent=2) + "\n")
    paths = [output / "stdout.log", output / "stderr.log"]
    with paths[0].open("wb") as stdout, paths[1].open("wb") as stderr:
      child = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, preexec_fn=no_core)
      try:
        if case.phase in ["wait", "recover"]:
          waiting(child, paths)
        if case.phase == "recover":
          cp.rmdir()
          cp.write_bytes(car_params().to_bytes())
        if case.phase in ["ready", "recover", "float"]:
          peers_ready(publisher, child)
          if case.phase != "float":
            subscribers = {name: messaging.sub_sock(name, conflate=False, timeout=0) for name in OUTPUTS}
          result["publications"] = frames(publisher, subscribers, child, output, 3 if subscribers else 1)
        if case.phase in ["integer", "float", "malformed"]:
          expected = (1 if mode == "rust" else -signal.SIGABRT) if case.phase != "malformed" else 1
          result["returncode"] = child.wait(timeout=15)
        else:
          child.send_signal(case.signal)
          result["returncode"] = child.wait(timeout=5)
          expected = (-signal.SIGINT if mode == "source" else 0) if case.phase == "wait" or case.signal == signal.SIGINT else -signal.SIGTERM
        result["expected_returncode"] = expected
        assert result["returncode"] == expected, result
        assert not Path(f"/proc/{child.pid}").exists()
        result["ok"] = True
      except (AssertionError, RuntimeError, TimeoutError, subprocess.TimeoutExpired) as error:
        result["error"] = repr(error)
        result["returncode"] = child.poll()
      finally:
        if child.poll() is None:
          child.kill()
          child.wait(timeout=5)
    subscribers.clear()
    del publisher
    gc.collect()
  (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")
  return result
