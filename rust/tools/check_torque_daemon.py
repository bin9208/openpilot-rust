#!/usr/bin/env python3
"""Real torqued IPC, cache restore, asynchronous persistence, cadence and signal QA."""

from __future__ import annotations
import argparse
import fcntl
import json
from pathlib import Path
import signal
import subprocess
import time
from torque_ipc import peer
from torque_reference import compare
from openpilot.cereal import log, messaging


def saved_points(per_bucket: int = 510) -> bytes:
  event = log.Event.new_message()
  cache = event.init("liveTorqueParameters")
  cache.version, cache.decay, cache.liveValid = 1, 50.0, True
  cache.latAccelFactorFiltered, cache.frictionCoefficientFiltered = 2.0, 0.12
  # Exactly representable collinear data permits entropy-seeded fits to share an oracle.
  cache.points = [[x, 2 * x] for x in [-0.375, -0.25, -0.125, -0.0625, 0.0625, 0.125, 0.25, 0.375] for _ in range(per_bucket)]
  return event.to_bytes()


def cadence(binary: Path, numerics: Path, output: Path) -> dict:
  with peer(binary, numerics, output / "cadence", {"simulation": False, "explicit_root": False}) as client:
    assert not client.start()["valid"]
    started = time.monotonic()
    for index in range(1, 31):
      time.sleep(max(0.0, started + index * 0.05 - time.monotonic()))
      client.step(index, valid=not 15 <= index <= 20)
    intervals = [(b - a) / 1e9 for a, b in zip(client.timestamps, client.timestamps[1:], strict=False)]
    assert all(0.18 < v < 0.35 for v in intervals), intervals
    for _ in range(5):
      time.sleep(0.1)
      client.source_step([])
    assert not client.receive(client.sent[-1])["valid"]
    silence = (client.timestamps[-1] - client.timestamps[-2]) / 1e9
    assert 0.4 < silence < 0.7
    return {
      "intervals_seconds": intervals,
      "silence_seconds": silence,
      "fields": client.fields,
      "queues": client.queue_files,
      "signal_seconds": client.signal(signal.SIGTERM),
    }


def persistence(binary: Path, numerics: Path, output: Path) -> dict:
  with peer(binary, numerics, output / "persistence", {"saved": saved_points()}) as client:
    assert client.start()["liveTorqueParameters"]["liveValid"]
    lock = (client.root / ".lock").open("rb")
    try:
      blocked_start = 0.0
      for index in range(1, 246):
        if index == 230:
          fcntl.flock(lock, fcntl.LOCK_EX)
          blocked_start = time.monotonic()
          previous = (client.directory / "LiveTorqueParameters").read_bytes()
        time.sleep(0.005)
        client.step(index)
        if index == 245:
          assert (client.directory / "LiveTorqueParameters").read_bytes() == previous
          blocked_elapsed = time.monotonic() - blocked_start
          assert blocked_elapsed < 2
          signal_start = time.monotonic()
          client.process.send_signal(signal.SIGTERM)
          deadline = signal_start + 0.5
          while time.monotonic() < deadline and Path(f"/proc/{client.process.pid}/wchan").read_text().strip() != "futex_do_wait":
            time.sleep(0.005)
          assert Path(f"/proc/{client.process.pid}/wchan").read_text().strip() == "futex_do_wait", "signal did not stop IPC loop and join blocked writer"
          loop_stop = time.monotonic() - signal_start
          assert loop_stop < 0.5
          for _ in range(10):
            client.publishers["livePose"].send(messaging.new_message("livePose").to_bytes())
            time.sleep(0.01)
          assert client.subscriber.receive(non_blocking=True) is None, "IPC publication continued after signal"
          assert client.process.poll() is None, "durable write was abandoned while lock was held"
          fcntl.flock(lock, fcntl.LOCK_UN)
      # Harness timeout only; source-required filesystem draining has no signal-time guarantee.
      assert client.process.wait(timeout=10) == 0
      data = (client.directory / "LiveTorqueParameters").read_bytes()
      (client.destination / "saved.capnp").write_bytes(data)
      with log.Event.from_bytes(data) as actual, log.Event.from_bytes(client.params.writes[-1]) as expected:
        wanted = expected.to_dict()
        wanted["logMonoTime"] = actual.logMonoTime
        fields = compare(actual.to_dict(), wanted)
        assert actual.liveTorqueParameters.totalBucketPoints > 4080
      assert len(client.params.writes) == 2
      return {
        "frames": 245,
        "fields": client.fields,
        "persisted_fields": fields,
        "blocked_write_window_seconds": blocked_elapsed,
        "signal_to_loop_stop_seconds": loop_stop,
        "post_signal_publications": 0,
        "writes": len(client.params.writes),
      }
    finally:
      fcntl.flock(lock, fcntl.LOCK_UN)
      lock.close()


def lifecycle(binary: Path, numerics: Path, output: Path) -> list[dict]:
  results = []
  for signum in (signal.SIGINT, signal.SIGTERM):
    for started in (False, True):
      with peer(binary, numerics, output / f"{signum.name}-{started}", {}) as client:
        if started:
          client.start()
          deadline = time.monotonic() + 10
          while time.monotonic() < deadline:
            workers = [p for p in Path(f"/proc/{client.process.pid}/task").glob("*/wchan") if p.parent.name != str(client.process.pid)]
            if (client.directory / "LiveTorqueParameters").exists() and workers and all(p.read_text().strip() == "futex_do_wait" for p in workers):
              break
            time.sleep(0.01)
          else:
            raise TimeoutError("initial durable write did not complete before idle-signal scenario")
        else:
          time.sleep(0.15)
          assert client.subscriber.receive(non_blocking=True) is None
          assert client.process.poll() is None
        results.append({"signal": signum.name, "waiting": "IPC" if started else "CarParams", "exit": 0, "seconds": client.signal(signum)})
  for label, saved in (("empty", None), ("restore", saved_points()), ("corrupt", b"invalid torque cache"), ("debug-sparse", saved_points(1))):
    with peer(binary, numerics, output / f"bounded-{label}", {"frames": 1, "saved": saved, "debug": label == "debug-sparse"}) as client:
      packet = client.start()
      assert packet["liveTorqueParameters"]["liveValid"] == (label == "restore")
      assert client.process.wait(timeout=10) == 0
      data = (client.directory / "LiveTorqueParameters").read_bytes()
      (client.destination / "saved.capnp").write_bytes(data)
      with log.Event.from_bytes(data) as actual, log.Event.from_bytes(client.params.writes[-1]) as expected:
        wanted = expected.to_dict()
        wanted["logMonoTime"] = actual.logMonoTime
        fields = compare(actual.to_dict(), wanted)
      results.append({"bounded": label, "fields": fields, "exit": 0})
  return results


def check(binary: Path, numerics: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  failures = []
  for arguments in (["--frames", "0"], ["--frames"], ["--unknown"], ["--numerics", str(output / "missing")]):
    result = subprocess.run([binary, *arguments], capture_output=True, timeout=3)
    assert result.returncode == 1
    failures.append({"arguments": arguments, "exit": 1, "stderr": result.stderr.decode()})
  report = {
    "cli": failures,
    "cadence": cadence(binary, numerics, output),
    "persistence": persistence(binary, numerics, output),
    "lifecycle": lifecycle(binary, numerics, output),
    "result": "pass",
    "scope": "host native IPC and original source oracle, entropy RNG with exact collinear fixtures; no target/device or CPU-saving acceptance",
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report, indent=2))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--numerics", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.numerics.resolve(), args.output.resolve())
