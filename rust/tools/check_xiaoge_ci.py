#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


@dataclass(frozen=True, slots=True)
class Check:
  name: str
  script: str
  arguments: tuple[str, ...]


def main() -> None:
  parser = argparse.ArgumentParser(description="Run source-backed Xiaoge policy, inference and actual daemon checks on the current architecture.")
  for name in ["bin-dir", "models", "opencv", "binding", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  for name in ["bin_dir", "models", "opencv", "binding", "output"]:
    setattr(args, name, getattr(args, name).resolve())
  if shutil.disk_usage(args.output.parent).free < 25 * 1024**3 + 512 * 1024**2:
    raise OSError("Xiaoge CI requires 25 GiB free plus 512 MiB; recover 35 GiB before resuming")
  args.output.mkdir()
  examples = args.bin_dir / "examples"
  common = ("--binary", str(args.bin_dir / "openpilot-xiaoge"), "--binding", str(args.binding),
            "--assets", str(args.models), "--opencv", str(args.opencv))
  checks = [
    Check("policy", "check_xiaoge_policy.py", ("--binary", str(examples / "xiaoge_policy"))),
    Check("lane", "check_xiaoge_lane.py", ("--binary", str(examples / "xiaoge_lane_trace"))),
    Check("inference", "check_xiaoge_inference.py", ("--binary", str(examples / "xiaoge_inference_trace"), "--models", str(args.models))),
    Check("opencv-reference", "generate_xiaoge_opencv_reference.py", ("--models", str(args.models))),
    Check("opencv", "check_xiaoge_opencv.py", ("--binary", str(examples / "opencv_trace"),
      "--fixtures", str(args.output / "opencv-reference/fixtures.json"))),
    Check("jpeg", "check_xiaoge_opencv_jpeg.py", ("--binary", str(examples / "jpeg_options"))),
    Check("http-tcp", "check_xiaoge_runtime.py", common),
    Check("live", "check_xiaoge_live.py", common),
    Check("lifecycle", "check_xiaoge_lifecycle.py", common),
  ]
  commands = []
  for check in checks:
    command = [sys.executable, "-u", "-P", str(ROOT / "rust/tools" / check.script),
               *check.arguments, "--output", str(args.output / check.name)]
    with (args.output / (check.name + ".log")).open("w") as log:
      run = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=False)
    commands.append({"name": check.name, "argv": command, "returncode": run.returncode})
    (args.output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    print(check.name, run.returncode, flush=True)
    run.check_returncode()
  binaries = [args.bin_dir / "openpilot-xiaoge", args.binding, examples / "opencv_trace", examples / "jpeg_options",
              *[examples / name for name in ["xiaoge_policy", "xiaoge_lane_trace", "xiaoge_inference_trace"]]]
  result = {"status": "PASS", "architecture": os.uname().machine, "checks": [check.name for check in checks],
    "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries},
    "scope": "source and native process comparisons on the named host architecture; no device acceptance or CPU claim"}
  (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
