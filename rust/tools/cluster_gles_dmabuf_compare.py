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
from typing import TypedDict

from cluster_gles_readback_compare import ComparisonFailure, ProcessFailure
from cluster_gles_dmabuf_source import Record


class Summary(TypedDict):
  textures: int
  framebuffers: int
  images: int
  fences: int
  imports: int
  destroys: int
  attributes_ok: int
  destroy_fd_live: int
  bindings: list[int]


class Observation(TypedDict):
  records: list[Record]
  summary: Summary | None


def observations(text: str) -> Observation:
  records = next(json.loads(line) for line in text.splitlines() if line.startswith("["))
  summary = next((json.loads(line)["provider_summary"] for line in text.splitlines() if line.startswith('{"provider_summary":')), None)
  return {"records": records, "summary": summary}


def cleanup_correction(source: Observation, native: Observation) -> bool:
  error = "GLES error state did not clear before encoder DMA-BUF import"
  summary = source["summary"]
  if source["records"] != [{"import_error": error}, {"fd_delta": 1}] or summary is None:
    return False
  if summary["images"] != 1 or summary["destroys"] != 0:
    return False
  expected = {"records": [{"import_error": error}, {"fd_delta": 0}], "summary": dict(summary, images=0, destroys=1)}
  return native == expected


def main() -> None:
  native, output = (Path(value).resolve() for value in sys.argv[1:])
  root = Path(__file__).resolve().parents[2]
  output.mkdir(parents=True, exist_ok=True)
  provider = output / "libowneddma.so"
  command = ["cc", "-std=c11", "-shared", "-fPIC", "-O0", str(root / "rust/tools/fixtures/cluster_gles_dmabuf_provider.c"), "-o", str(provider)]
  free = shutil.disk_usage(output).free
  if free < 25 * 1024**3 + 1024**2:
    raise ProcessFailure("disk floor", output)
  with (output / "build-provider.log").open("w") as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=30, check=True)
  commands = [{"command": command, "free_bytes": free, "returncode": 0}]
  cases = []
  for scenario in ("success", "no-display", "no-extension", "missing-functions", "import-reject", "uncleared", "bind-error", "framebuffer-error", "framebuffer-incomplete", "fence-zero", "timeout", "wait-error", "unexpected", "host"):
    library = "HOST" if scenario == "host" else str(provider)
    outputs = []
    for name, invocation in (("source", [sys.executable, str(Path(__file__).with_name("cluster_gles_dmabuf_source.py"))]), ("native", [str(native)])):
      destination = output / f"{scenario}-{name}"
      invocation.extend([library, str(destination)])
      environment = dict(os.environ, OWNED_DMA_SCENARIO=scenario)
      result = subprocess.run(invocation, cwd=root, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=20, check=False)
      log = output / f"{scenario}-{name}.log"
      log.write_text(result.stdout)
      commands.append({"command": invocation, "scenario": scenario, "returncode": result.returncode, "log": str(log)})
      if result.returncode:
        raise ProcessFailure(name, log)
      outputs.append(observations(result.stdout))
    source, actual = outputs
    exact = source == actual
    correction = cleanup_correction(source, actual)
    cases.append({"scenario": scenario, "exact": exact, "explicit_source_leak_correction": correction, "source": source, "native": actual})
  report = {"commands": commands, "native_sha256": hashlib.sha256(native.read_bytes()).hexdigest(), "provider_sha256": hashlib.sha256(provider.read_bytes()).hexdigest(), "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_gles_dmabuf.py").read_bytes()).hexdigest(), "cases": cases}
  path = output / "comparison.json"
  path.write_text(json.dumps(report, indent=2) + "\n")
  if not all(case["exact"] or case["explicit_source_leak_correction"] for case in cases):
    raise ComparisonFailure(path)
  print(f"{len(cases)} DMA source/native controls passed")


if __name__ == "__main__":
  main()
