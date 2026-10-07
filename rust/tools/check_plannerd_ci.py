#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
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
TOOLS = ROOT / "rust/tools"


@dataclass(frozen=True, slots=True)
class Check:
  name: str
  script: Path
  arguments: tuple[str, ...] = ()
  output_option: str = "--output"


@dataclass(frozen=True, slots=True)
class Inputs:
  binaries: Path
  artifact: Path
  source: Path
  binding: Path


class InsufficientSpace(OSError):
  def __init__(self, free: int) -> None:
    self.free = free
    super().__init__(f"Planner comparison requires 25 GiB plus 512 MiB; free={free}; recover 35 GiB before resuming")


def run_checks(checks: tuple[Check, ...], output: Path) -> None:
  output.mkdir(parents=True, exist_ok=False)
  commands = []
  for check in checks:
    free = shutil.disk_usage(output).free
    if free < 25 * 1024**3 + 512 * 1024**2:
      raise InsufficientSpace(free)
    command = [sys.executable, "-u", "-P", str(check.script), *check.arguments,
               check.output_option, str(output / check.name)]
    with (output / (check.name + ".log")).open("w") as log:
      result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=False)
    commands.append({"name": check.name, "argv": command, "returncode": result.returncode, "free_before": free})
    (output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    print(check.name, result.returncode, flush=True)
    result.check_returncode()


def runtime_checks(inputs: Inputs) -> tuple[Check, ...]:
  examples = inputs.binaries / "examples"
  owner = ("--source-native", str(inputs.source), "--artifact", str(inputs.artifact))
  runtime = (*owner, "--binary", str(examples / "plannerd_runtime_trace"))
  daemon = (*owner, "--python", sys.executable, "--binding", str(inputs.binding),
            "--binary", str(inputs.binaries / "openpilot-plannerd"))
  return (
    Check("abi", TOOLS / "plannerd_acados_source.py", ("--baseline", str(inputs.source), "--artifact", str(inputs.artifact),
      "--trace", str(examples / "plannerd_solver_trace")), "--evidence"),
    Check("policy", TOOLS / "plannerd_policy_source.py", ("--trace", str(examples / "plannerd_policy_trace")), "--evidence"),
    Check("radar", TOOLS / "plannerd_radar_source.py", ("--binary", str(examples / "plannerd_radar_trace"))),
    Check("gap", TOOLS / "plannerd_gap_source.py", ("--binary", str(examples / "plannerd_gap_trace"))),
    Check("owner", TOOLS / "plannerd_owner_source.py", (*owner, "--binary", str(examples / "plannerd_owner_trace"))),
    Check("main", TOOLS / "plannerd_runtime_source.py", runtime),
    Check("fault", TOOLS / "plannerd_runtime_source.py", (*runtime, "--faults", "--frames", "80")),
    Check("volkswagen", TOOLS / "plannerd_runtime_source.py", (*runtime, "--brand", "volkswagen")),
    Check("stock-longitudinal", TOOLS / "plannerd_runtime_source.py", (*runtime, "--stock-longitudinal")),
    Check("radar-off", TOOLS / "plannerd_runtime_source.py", (*runtime, "--radar-mode", "0")),
    Check("ipc", TOOLS / "plannerd_ipc.py", daemon),
    Check("lifecycle", TOOLS / "check_plannerd_lifecycle.py", daemon),
  )


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare the actual original and native planner on this host architecture.")
  for name in ("bin-dir", "artifact", "source-native", "binding", "output"):
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  inputs = Inputs(args.bin_dir.resolve(), args.artifact.resolve(), args.source_native.resolve(), args.binding.resolve())
  output = args.output.resolve()
  checks = runtime_checks(inputs)
  run_checks(checks, output)
  binaries = [inputs.binaries / "openpilot-plannerd", inputs.binding, inputs.artifact / "manifest.json"]
  binaries += [inputs.binaries / "examples" / ("plannerd_" + name + "_trace")
               for name in ("solver", "policy", "radar", "gap", "owner", "runtime")]
  receipt = {"status": "PASS", "architecture": os.uname().machine, "checks": [check.name for check in checks],
    "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries},
    "scope": "exact source policies, main loops, native IPC and lifecycle on the stated host; no device acceptance"}
  (output / "result.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps(receipt))


if __name__ == "__main__":
  main()
