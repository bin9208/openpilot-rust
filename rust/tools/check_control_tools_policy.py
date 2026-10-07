#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ("binary", "source", "output"):
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  request = (args.source / "input.json").read_bytes()
  expected = json.loads((args.source / "expected.json").read_bytes())
  command = [str(args.binary.resolve())]
  result = subprocess.run(command, input=request, capture_output=True, check=False)
  (args.output / "stdout.json").write_bytes(result.stdout)
  (args.output / "stderr.log").write_bytes(result.stderr)
  result.check_returncode()
  actual = json.loads(result.stdout)
  assert len(actual) == len(expected)
  mismatches = []
  for case, (left, right) in enumerate(zip(expected, actual, strict=True)):
    assert len(left) == len(right), (case, len(left), len(right))
    for step, (a, b) in enumerate(zip(left, right, strict=True)):
      if a != b:
        mismatches.append({"case": case, "step": step, "expected": a, "actual": b})
  receipt = {"cases": len(actual), "steps": sum(map(len, actual)), "mismatch_count": len(mismatches),
    "mismatches": mismatches[:30], "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "input_sha256": hashlib.sha256(request).hexdigest(), "command": command}
  (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps(receipt))
  assert not mismatches, "control tool source states differ"


if __name__ == "__main__":
  main()
