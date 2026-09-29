#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]


def probe(binary: Path, frames: int, interval_ms: int) -> dict[str, str | int | float | list[int]]:
  command = [str(binary)]
  source_commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
  source_dirty = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "rust", "openpilot/cereal", "msgq_repo", "openpilot/common/params.h", "openpilot/common/params_keys.h"], cwd=ROOT).returncode != 0
  artifact_source = binary.with_name("SOURCE_COMMIT")
  artifact_commit = artifact_source.read_text().strip() if artifact_source.exists() else "unpackaged-local-build"
  if artifact_source.exists() and (artifact_commit != source_commit or source_dirty):
    raise RuntimeError("artifact SOURCE_COMMIT must match the clean checked-out dev source")
  with tempfile.TemporaryDirectory(prefix="msgq_rust-probe-", dir="/dev/shm") as temporary:
    prefix = Path(temporary).name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = prefix
    os.environ.pop("CEREAL_FAKE", None)
    from openpilot.cereal import messaging

    subprocess.run([*command, "--self-test"], check=True, timeout=15)
    subscriber = messaging.sub_sock("procLog", timeout=100)
    timestamps: list[int] = []
    process_counts: list[int] = []
    cpu_counts: list[int] = []
    with subprocess.Popen([*command, "--publish", "--frames", str(frames), "--interval-ms", str(interval_ms)]) as producer:
      try:
        deadline = time.monotonic() + 10 + frames * interval_ms / 1000
        while len(timestamps) < frames and time.monotonic() < deadline:
          message = messaging.recv_one(subscriber)
          if message is None:
            if producer.poll() is not None:
              break
            continue
          if message.which() != "procLog" or not message.valid:
            raise RuntimeError("invalid Event/procLog message")
          if timestamps and message.logMonoTime <= timestamps[-1]:
            raise RuntimeError("non-increasing monotonic timestamps")
          if not message.procLog.procs or not message.procLog.cpuTimes or message.procLog.mem.total <= 0:
            raise RuntimeError("empty process/CPU/memory collection")
          if not any(process.pid == producer.pid for process in message.procLog.procs):
            raise RuntimeError("producer PID missing from procLog")
          timestamps.append(message.logMonoTime)
          process_counts.append(len(message.procLog.procs))
          cpu_counts.append(len(message.procLog.cpuTimes))
        if len(timestamps) != frames:
          raise RuntimeError(f"received {len(timestamps)}/{frames} procLog messages")
        if producer.wait(timeout=5) != 0:
          raise RuntimeError("Rust producer failed")
      finally:
        if producer.poll() is None:
          producer.terminate()
          try:
            producer.wait(timeout=3)
          except subprocess.TimeoutExpired:
            producer.kill()
            producer.wait(timeout=3)
    del subscriber
    intervals = [(b - a) / 1e6 for a, b in zip(timestamps, timestamps[1:])]
    return {
      "result": "pass",
      "scope": "isolated procLog IPC and temporary Params; production runtime unchanged",
      "execution": "native",
      "host_architecture": platform.machine(),
      "kernel": platform.release(),
      "source_commit": source_commit,
      "source_dirty": source_dirty,
      "artifact_source_commit": artifact_commit,
      "binary_sha256": binary_digest(binary),
      "frames": len(timestamps),
      "process_counts": process_counts,
      "cpu_counts": cpu_counts,
      "requested_interval_ms": interval_ms,
      "minimum_interval_ms": min(intervals, default=0),
      "maximum_interval_ms": max(intervals, default=0),
    }


def binary_digest(binary: Path) -> str:
  with binary.open("rb") as source:
    return hashlib.file_digest(source, "sha256").hexdigest()


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Bounded isolated Rust IPC/Params probe; never changes production process selection")
  parser.add_argument("--binary", required=True, type=Path)
  parser.add_argument("--frames", type=int, default=10)
  parser.add_argument("--interval-ms", type=int, default=2000)
  parser.add_argument("--report", required=True, type=Path)
  args = parser.parse_args()
  if not 2 <= args.frames <= 30 or not 10 <= args.interval_ms <= 2000:
    parser.error("frames must be 2..30 and interval-ms 10..2000")
  with args.report.open("x") as output:
    try:
      result = probe(args.binary.resolve(), args.frames, args.interval_ms)
    except Exception as error:
      result = {"result": "fail", "error": str(error), "device_runtime_changed": False}
      json.dump(result, output, indent=2)
      output.write("\n")
      print(json.dumps(result, indent=2), file=sys.stderr)
      sys.exit(1)
    json.dump(result, output, indent=2)
    output.write("\n")
  print(json.dumps(result, indent=2))
