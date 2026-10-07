#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import signal

from plannerd_lifecycle_peer import Case, run


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare actual planner Params faults, recovery, signal phases and fatal conversion boundaries.")
  for name in ["python", "binary", "artifact", "binding", "source-native", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  for name in ["python", "binary", "artifact", "binding", "source_native", "output"]:
    setattr(args, name, getattr(args, name).resolve())
  assert shutil.disk_usage(args.output.parent).free >= 25 * 1024**3 + 128 * 1024**2
  args.output.mkdir(parents=True, exist_ok=False)
  cases = [Case(f"wait-{state}-{name}", "wait", value, state) for state in ["missing", "directory", "empty"]
           for name, value in [("sigint", signal.SIGINT), ("sigterm", signal.SIGTERM)]]
  cases += [Case("ready-sigint", "ready", signal.SIGINT), Case("ready-sigterm", "ready"),
            Case("recover-directory", "recover", car_params="directory"),
            Case("settings-directories", "ready", directories=("EnableRadarTracks", "UseLaneLineSpeed", "LatMpcPathCost", "LongActuatorDelay")),
            Case("fatal-integer", "integer"), Case("fatal-float", "float"), Case("malformed-CarParams", "malformed")]
  results = []
  for case in cases:
    for mode in ["source", "rust"]:
      result = run(args, case, mode)
      results.append(result)
      (args.output / "cases.json").write_text(json.dumps(results, indent=2) + "\n")
      print(case.name, mode, result["ok"], flush=True)
  receipt = {"status": "PASS" if all(row["ok"] for row in results) else "FAIL", "cases": len(cases),
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "scope": "real source/native lifecycle; native clean SIGINT matches the original manager launcher",
    "exit_boundary": "raw source KeyboardInterrupt=-2; invalid numeric source abort vs typed Rust exit1 remains explicit"}
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps(receipt))
  assert receipt["status"] == "PASS", "planner lifecycle mismatches; see individual receipts"


if __name__ == "__main__":
  main()
