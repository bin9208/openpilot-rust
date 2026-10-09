from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time


def main() -> None:
  parser = argparse.ArgumentParser(description="Observe updater TERM while both actual commands wait for owned network readiness")
  for name in ("binary", "binding", "package", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  digest = sha256(args.binary.read_bytes()).hexdigest()
  rows = []
  with socket.socket() as reserved:
    reserved.bind(("localhost", 0))
    url = f"https://localhost:{reserved.getsockname()[1]}/manifest.json"
    for side in ("source", "native"):
      root = args.evidence / side
      (root / "params/d").mkdir(parents=True)
      (root / "params/d/UsbGpuHardwareSeen").write_text("1")
      (root / "devices").mkdir()
      env = dict(os.environ, PARAMS_ROOT=str(root / "params"), OPENPILOT_PREFIX="d", CARROT_BIG_MODEL_DIR=str(root / "cache"), PYTHONUNBUFFERED="1")
      if side == "source":
        argv = [sys.executable, "-P", str(Path(__file__).with_name("usbgpu_background_source.py"))]
        request = json.dumps({"root": str(root), "binding": str(args.binding), "url": url, "wait": 60})
      else:
        argv = [
          str(args.binary),
          "--ensure-if-egpu",
          "--root",
          str(root),
          "--devices",
          str(root / "devices"),
          "--assets",
          str(args.package),
          "--manifest-url",
          url,
          "--network-wait-seconds",
          "60",
        ]
        request = None
      with subprocess.Popen(argv, env=env, text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as process:
        try:
          assert process.stdin and process.stdout
          if request:
            process.stdin.write(request)
          process.stdin.close()
          process.stdin = None
          assert select.select([process.stdout], [], [], 4)[0]
          phase = process.stdout.readline()
          assert phase == "waiting up to 60s for the big model server\n", phase
          started = time.monotonic()
          process.terminate()
          forced = False
          try:
            stdout, stderr = process.communicate(timeout=1)
          except subprocess.TimeoutExpired:
            forced = True
            process.kill()
            stdout, stderr = process.communicate(timeout=2)
          row = {
            "side": side,
            "argv": argv,
            "request": request,
            "pid": process.pid,
            "phase": phase,
            "exit": process.returncode,
            "stdout": stdout,
            "stderr": stderr,
            "forced_cleanup": forced,
            "seconds": time.monotonic() - started,
          }
          rows.append(row)
          (args.evidence / "result.json").write_text(json.dumps({"rows": rows, "binary_sha256": digest}, indent=2) + "\n")
        finally:
          if process.poll() is None:
            process.kill()
            process.wait(timeout=2)
  assert sha256(args.binary.read_bytes()).hexdigest() == digest
  assert all(row["exit"] == -15 and not row["forced_cleanup"] for row in rows), rows
  print(json.dumps({"cases": 1, "term_reaped": True}))


if __name__ == "__main__":
  main()
