# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import ctypes
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def main() -> None:
  native, output = (Path(value).resolve() for value in sys.argv[1:])
  root = Path(__file__).resolve().parents[2]
  output.mkdir(parents=True, exist_ok=True)
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  assert libc.prctl(36, 1, 0, 0, 0) == 0
  provider = output / "fc-list"
  provider.write_text('#!/bin/sh\n/bin/sleep 30 &\necho "$!" > "$OWNED_FONT_PID"\nexit 0\n')
  provider.chmod(0o755)
  pid_file = output / "descendant.pid"
  environment = dict(os.environ, PATH=f"{output}:{os.environ.get('PATH', '')}", OWNED_FONT_PID=str(pid_file))
  command = [str(native), "render", str(root / ".omo/evidence/253-cluster-replay-tools/blocked-font/cases.json"), str(output / "native"), str(root / ".omo/evidence/253-cluster-replay-tools/gui-config.json")]
  started = time.monotonic()
  result = subprocess.run(command, env=environment, capture_output=True, timeout=5, check=False)
  elapsed = time.monotonic() - started
  (output / "native.log").write_bytes(result.stdout + result.stderr)
  pid = int(pid_file.read_text())
  waited, status = os.waitpid(pid, os.WNOHANG)
  alive = waited == 0
  if alive:
    os.kill(pid, signal.SIGKILL)
    waited, status = os.waitpid(pid, 0)
  report = dict(command=command, environment={"PATH": environment["PATH"], "OWNED_FONT_PID": str(pid_file), "DISPLAY": environment.get("DISPLAY")}, exit_code=result.returncode, elapsed=elapsed, descendant_pid=pid, descendant_alive_after_return=alive, cleanup_waited_pid=waited, cleanup_status=status, native_sha256=hashlib.sha256(native.read_bytes()).hexdigest())
  (output / "result.json").write_text(json.dumps(report, indent=2))
  print(json.dumps(report))
  if result.returncode or alive or elapsed > 2:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
