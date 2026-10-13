# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from cluster_gles_readback_error_source import Scenario
from cluster_gles_readback_compare import ComparisonFailure, ProcessFailure


def main() -> None:
  native, output = (Path(value).resolve() for value in sys.argv[1:])
  root = Path(__file__).resolve().parents[2]
  output.mkdir(parents=True, exist_ok=True)
  providers = []
  commands = []
  for name, flags in (("complete", []), ("missing", ["-DOWNED_GL_MISSING_ASYNC"])):
    provider = output / f"libownedgles-{name}.so"
    command = ["cc", "-std=c11", "-shared", "-fPIC", "-O0", *flags, str(root / "rust/tools/fixtures/cluster_gles_readback_provider.c"), "-o", str(provider)]
    free = shutil.disk_usage(output).free
    if free < 25 * 1024**3 + 1024**2:
      raise ProcessFailure("disk floor", output)
    with (output / f"build-{name}.log").open("w") as log:
      result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=30, check=False)
    commands.append({"command": command, "free_bytes": free, "returncode": result.returncode})
    if result.returncode:
      raise ProcessFailure(name, output / f"build-{name}.log")
    providers.append(provider)
  comparisons = []
  for scenario in Scenario:
    provider = providers[1] if scenario is Scenario.MISSING_ASYNC else providers[0]
    observations = []
    for name, invocation in (("source", [sys.executable, str(Path(__file__).with_name("cluster_gles_readback_error_source.py")), str(provider)]), ("native", [str(native), str(provider)])):
      environment = dict(os.environ, OWNED_GL_SCENARIO=scenario.value)
      result = subprocess.run(invocation, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=10, check=False)
      log = output / f"{scenario.value}-{name}.log"
      log.write_text(result.stdout)
      commands.append({"command": invocation, "scenario": scenario.value, "returncode": result.returncode, "log": str(log)})
      if result.returncode:
        raise ProcessFailure(name, log)
      observations.append([json.loads(line) for line in result.stdout.splitlines() if line.startswith(("[", "{"))])
    comparisons.append({"scenario": scenario.value, "exact": observations[0] == observations[1], "source": observations[0], "native": observations[1]})
  report = {"native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "provider_sha256": [hashlib.sha256(path.read_bytes()).hexdigest() for path in providers], "commands": commands, "cases": comparisons}
  path = output / "comparison.json"
  path.write_text(json.dumps(report, indent=2) + "\n")
  if not all(case["exact"] for case in comparisons):
    raise ComparisonFailure(path)
  print(f"{len(comparisons)} source/native failure controls passed")


if __name__ == "__main__":
  main()
