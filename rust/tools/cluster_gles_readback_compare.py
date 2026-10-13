# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


class ProcessFailure(RuntimeError):
  def __init__(self, name: str, log: Path) -> None:
    self.name, self.log = name, log
    super().__init__(f"{name} failed: {log}")


class ComparisonFailure(RuntimeError):
  def __init__(self, report: Path) -> None:
    self.report = report
    super().__init__(f"GLES source/native comparison failed: {report}")


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True, exist_ok=True)
  invocations = []
  for name, command in (
    ("source", [sys.executable, str(Path(__file__).with_name("cluster_gles_readback_source.py"))]),
    ("native", [str(args.native.resolve())]),
  ):
    destination = args.output / name
    command.append(str(destination.resolve()))
    with (args.output / f"{name}.log").open("w") as log:
      result = subprocess.run(command, cwd=root, env=os.environ.copy(), stdout=log, stderr=subprocess.STDOUT, timeout=30, check=False)
    invocations.append({"command": command, "returncode": result.returncode})
    if result.returncode != 0:
      raise ProcessFailure(name, args.output / f"{name}.log")
  source = args.output / "source"
  native = args.output / "native"
  source_records = json.loads((source / "result.json").read_text())
  native_records = json.loads((native / "result.json").read_text())
  byte_cases = []
  for name, width, height, frame in (
    ("direct-0", 32, 24, 0), ("copy-1", 32, 24, 1), ("copy-2", 32, 24, 2),
    ("copy-3", 32, 24, 3), ("copy-resized", 16, 12, 6), ("direct-disabled", 16, 12, 7),
  ):
    expected = bytes(value for y in range(height - 1, -1, -1) for x in range(width) for value in ((x * 7 + frame * 13) % 256, y * 11 % 256, (x + y) * 4 % 256, 255)) + b"\xA5" * 7
    original = (source / f"{name}.rgba").read_bytes()
    actual = (native / f"{name}.rgba").read_bytes()
    byte_cases.append({"name": name, "bytes": len(actual), "exact": original == actual == expected, "sha256": hashlib.sha256(actual).hexdigest()})
  result = {
    "invocations": invocations,
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_gles_readback.py").read_bytes()).hexdigest(),
    "records": len(native_records), "records_exact": source_records == native_records,
    "all_binding_restorations": all(record.get("restored", True) for record in native_records),
    "buffers": byte_cases,
  }
  (args.output / "comparison.json").write_text(json.dumps(result, indent=2) + "\n")
  if not result["records_exact"] or not result["all_binding_restorations"] or not all(item["exact"] for item in byte_cases):
    raise ComparisonFailure(args.output / "comparison.json")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
