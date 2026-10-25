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
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
MEMORY_CHECKS = ('abi', 'owner', 'fault')


def native_command(args, wrappers):
  command = [str(args.controller.resolve(strict=True)), 'memory', '--root', str(ROOT),
             '--python', str(args.python.absolute()), '--bin-dir', str(wrappers),
             '--artifact', str(args.artifact.resolve()), '--source-native', str(args.source_native.resolve()),
             '--binding', str(args.binding.resolve()), '--output', str(args.output.resolve() / 'comparison'),
             '--timeout-seconds', str(args.timeout_seconds)]
  for growth in args.growth_mib:
    command.extend(('--growth-mib', growth))
  return command


def main() -> None:
  parser = argparse.ArgumentParser(description="Exercise generated planner C with ASan/UBSan and the unchanged original source.")
  for name in ("bin-dir", "artifact", "source-native", "binding", "asan-library", "output"):
    parser.add_argument("--" + name, type=Path, required=True)
  parser.add_argument('--controller', type=Path, required=True)
  parser.add_argument('--python', type=Path, default=Path(sys.executable))
  parser.add_argument('--timeout-seconds', type=int, required=True)
  parser.add_argument('--growth-mib', action='append', required=True)
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
  completed = subprocess.run(native_command(args, wrappers.parent), cwd=ROOT, check=False)
  if completed.returncode != 0:
    raise SystemExit(completed.returncode if completed.returncode > 0 else 1)
  commands = json.loads((output / 'comparison/commands.json').read_text())
  assert [(row['name'], row['returncode']) for row in commands] == [(name, 0) for name in MEMORY_CHECKS], commands
  result = {"status": "PASS", "checks": list(MEMORY_CHECKS), "binary_sha256": identities,
    "artifact_sha256": hashlib.sha256((artifact / "manifest.json").read_bytes()).hexdigest(),
    "asan_sha256": hashlib.sha256(asan.read_bytes()).hexdigest(),
    "scope": "generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented"}
  (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
