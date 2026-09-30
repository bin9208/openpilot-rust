#!/usr/bin/env python3
"""Reproduce the inherited DEBUG points packet size failure in original and Rust IPC."""

from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
from check_torque_daemon import saved_points
from torque_ipc import peer
from torque_cases import car_bytes


def check(binary: Path, numerics: Path, output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  data = saved_points()
  payload = output / "source-points.capnp"
  payload.write_bytes(data)
  with peer(binary, numerics, output / "rust", {"saved": data, "debug": True, "frames": 1}) as client:
    client.put("CarParams", car_bytes())
    assert client.process.wait(timeout=3) == 1
    rust_error = (client.destination / "daemon.log").read_text()
    assert "fitting one third of the queue" in rust_error
    child = Path(__file__).with_name('torque_capacity_child.py').resolve()
    try:
      original = subprocess.run([sys.executable, str(child), str(payload)], env=dict(os.environ), cwd="/tmp", capture_output=True, timeout=5)
    except subprocess.TimeoutExpired as error:
      (output / "source-stderr.log").write_bytes(error.stderr or b'')
      (output / "source-stages.jsonl").write_bytes(error.stdout or b'')
      raise
    (output / "source-stderr.log").write_bytes(original.stderr)
    (output / "source-stages.jsonl").write_bytes(original.stdout)
    assert original.returncode == -signal.SIGABRT, (original.returncode, original.stderr)
    stages = [json.loads(line) for line in original.stdout.splitlines()]
    assert [item['stage'] for item in stages] == ['before_import', 'after_import', 'before_send'], stages
  report = {
    "result": "pass",
    "inherited_defect": True,
    "points": 4080,
    "packet_bytes": len(data),
    "queue_bytes": 250 * 1024,
    "source_returncode": original.returncode,
    "rust_returncode": 1,
    "rust_error": rust_error,
    "source_guard": "msgq_repo/msgq/msgq.cc:246 assert(3 * total_msg_size <= q->size)",
    "source_stages": stages,
    "source_core_dump": "disabled with PR_SET_DUMPABLE=0; SIGABRT and original size guard unchanged",
    "scope": "preserved failure, not a fix",
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
