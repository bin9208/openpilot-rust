#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from check_proclog_reference import make_process
from device_probe import binary_digest


def check(binary: Path, runner: Path, report: Path) -> None:
  header = subprocess.check_output(["readelf", "-h", str(binary)], text=True)
  segments = subprocess.check_output(["readelf", "-l", str(binary)], text=True)
  dynamic = subprocess.check_output(["readelf", "-d", str(binary)], text=True)
  if "AArch64" not in header or "INTERP" in segments or "NEEDED" in dynamic:
    raise RuntimeError("candidate must be a static AArch64 ELF")
  with tempfile.TemporaryDirectory(prefix="rust-aarch64-proc-") as temporary, \
       tempfile.TemporaryDirectory(prefix="msgq_rust-probe-", dir="/dev/shm") as namespace:
    root = Path(temporary)
    (root / "stat").write_text("cpu 1 2 3 4 5 6 7\ncpu0 100 2 30 400 5 6 7\n")
    (root / "meminfo").write_text("MemTotal: 128 kB\n")
    make_process(root, 123, 2000)
    os.environ["OPENPILOT_PREFIX"] = Path(namespace).name.removeprefix("msgq_")
    os.environ.pop("CEREAL_FAKE", None)
    from openpilot.cereal import messaging

    command = [str(runner), str(binary), "--proc-root", str(root)]
    subprocess.run([*command, "--self-test"], check=True, timeout=15)
    subscriber = messaging.sub_sock("procLog", timeout=100)
    frames = 0
    with subprocess.Popen([*command, "--publish", "--frames", "3", "--interval-ms", "100"]) as producer:
      try:
        deadline = time.monotonic() + 10
        while frames < 3 and time.monotonic() < deadline:
          message = messaging.recv_one(subscriber)
          if message is None:
            if producer.poll() is not None: break
            continue
          assert message.valid and message.which() == "procLog"
          assert message.procLog.mem.total == 128 * 1024
          assert len(message.procLog.cpuTimes) == 1
          assert len(message.procLog.procs) == 1
          process = message.procLog.procs[0]
          assert process.pid == 123 and process.memRss == 2000 * 4096
          assert process.memPss == 50 * 1024 and process.nice == -5
          assert process.name == "worker ) (a"
          frames += 1
        assert frames == 3, frames
        assert producer.wait(timeout=5) == 0
      finally:
        if producer.poll() is None:
          producer.kill()
          producer.wait(timeout=3)
    del subscriber
  result = {"result": "pass", "execution": "qemu-aarch64", "procfs": "synthetic fixture", "frames": frames,
            "binary_sha256": binary_digest(binary), "elf": "static AArch64; no interpreter or shared dependencies",
            "device_validation": "not_run"}
  report.write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result, indent=2))


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Static AArch64 ELF and emulated synthetic IPC/Params checks; not device validation")
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", type=Path, required=True)
  parser.add_argument("--report", type=Path, required=True)
  args = parser.parse_args()
  check(args.binary.resolve(), args.runner.resolve(), args.report)
