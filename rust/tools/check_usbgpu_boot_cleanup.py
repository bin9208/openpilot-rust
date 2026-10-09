"""Observe actual boot runner and worker identities through cooperative cancellation."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import select
import subprocess
import time

from usbgpu_boot_fixture import setup


def identity(pid: int) -> dict[str, int | str]:
  fields = Path(f"/proc/{pid}/stat").read_text().rsplit(") ", 1)[1].split()
  return {"pid": pid, "start_time": fields[19], "process_group": int(fields[2])}


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ("binary", "worker", "metadata", "installed", "package", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  args = parser.parse_args()
  fixture = setup(args.evidence, args.installed, args.worker, args.metadata)
  argv = [
    str(args.binary),
    "--boot",
    "--root",
    str(fixture.root),
    "--assets",
    str(args.package),
    "--devices",
    str(fixture.devices),
    "--identity-root",
    str(fixture.identity),
    "--worker",
    str(fixture.worker),
  ]
  env = dict(os.environ, CARROT_BIG_MODEL_DIR=str(fixture.cache), USBGPU_WORKER_TEST_TIMEOUT="1")
  pidfds: list[int] = []
  with subprocess.Popen(argv, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
    try:
      deadline = time.monotonic() + 30
      while not (fixture.logs / "1928x1208.jsonl").is_file() or len((fixture.logs / "1928x1208.jsonl").read_text().splitlines()) < 2:
        assert process.poll() is None and time.monotonic() < deadline
        time.sleep(0.005)
      runner = int(Path(f"/proc/{process.pid}/task/{process.pid}/children").read_text().strip())
      worker = int(Path(f"/proc/{runner}/task/{runner}/children").read_text().strip())
      observed = [identity(runner), identity(worker)]
      assert observed[0]["process_group"] == observed[1]["process_group"] == runner
      worker_args = Path(f"/proc/{worker}/cmdline").read_bytes().split(b"\0")
      shared = Path(os.fsdecode(worker_args[2]))
      assert shared.is_file()
      pidfds = [os.pidfd_open(pid) for pid in (runner, worker)]
      started = time.monotonic()
      process.terminate()
      stdout, stderr = process.communicate(timeout=5)
      assert process.returncode == 1
      assert len(select.select(pidfds, [], [], 2)[0]) == 2
      assert all(not Path(f"/proc/{pid}").exists() for pid in (runner, worker))
      assert not shared.exists()
      assert not (fixture.installed.parent / "rejected").exists()
      assert not (fixture.installed.parent / "last_failure.json").exists()
      result = {
        "argv": argv,
        "before": observed,
        "runner_reaped": True,
        "worker_reaped": True,
        "pidfds_readable": True,
        "shared_removed": True,
        "cancel_did_not_reject": True,
        "seconds": time.monotonic() - started,
        "exit": process.returncode,
        "stdout": stdout,
        "stderr": stderr,
      }
      (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
      print(json.dumps(result))
    finally:
      if process.poll() is None:
        process.terminate()
        try:
          process.wait(timeout=4)
        except subprocess.TimeoutExpired:
          process.kill()
          process.wait(timeout=2)
      for fd in pidfds:
        os.close(fd)


if __name__ == "__main__":
  main()
