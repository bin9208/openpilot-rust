#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shlex

from check_plannerd_ci import Inputs, run_checks, runtime_checks


def main() -> None:
  parser = argparse.ArgumentParser(description="Exercise generated planner C with ASan/UBSan and the unchanged original source.")
  for name in ("bin-dir", "artifact", "source-native", "binding", "asan-library", "output"):
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  artifact, output = args.artifact.resolve(), args.output.resolve()
  provenance = json.loads((artifact / "provenance.json").read_text())
  assert provenance["sanitizers"], "an instrumented generated-C artifact is required"
  asan = args.asan_library.resolve(strict=True)
  wrappers = output / "wrappers/examples"
  wrappers.mkdir(parents=True, exist_ok=False)
  identities = {}
  for name in ("solver", "owner", "runtime"):
    binary = (args.bin_dir / "examples" / ("plannerd_" + name + "_trace")).resolve(strict=True)
    command = ["env", "LD_PRELOAD=" + str(asan), "ASAN_OPTIONS=detect_leaks=1:abort_on_error=1:strict_string_checks=1",
               "UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1", str(binary)]
    wrapper = wrappers / binary.name
    wrapper.write_text('#!/bin/sh\nexec ' + shlex.join(command) + ' "$@"\n')
    wrapper.chmod(0o755)
    identities[str(binary)] = hashlib.sha256(binary.read_bytes()).hexdigest()
  inputs = Inputs(wrappers.parent, artifact, args.source_native.resolve(), args.binding.resolve())
  checks = tuple(check for check in runtime_checks(inputs) if check.name in ("abi", "owner", "fault"))
  run_checks(checks, output / "comparison")
  result = {"status": "PASS", "checks": [check.name for check in checks], "binary_sha256": identities,
    "artifact_sha256": hashlib.sha256((artifact / "manifest.json").read_bytes()).hexdigest(),
    "asan_sha256": hashlib.sha256(asan.read_bytes()).hexdigest(),
    "scope": "generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented"}
  (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
