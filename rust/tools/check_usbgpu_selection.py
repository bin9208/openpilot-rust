"""Compare original modeld selection with native drivingd and genuine generic assets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

from usbgpu_selection_source import exception_controls, present, source


RUNTIME_SHA = "a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb"
MODEL_SHA = "e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b"


def digest(path: Path) -> str:
  with path.open("rb") as stream:
    return hashlib.file_digest(stream, "sha256").hexdigest()


def setup(root: Path, model: Path, archive: Path, marker: dict, scenario: str) -> None:
  assert shutil.disk_usage(root.parent).free > (25 * 1024 + 5120) * 1024**2
  cache = root / "cache"
  target = cache / "precompiled" / MODEL_SHA
  target.mkdir(parents=True)
  (root / "models").mkdir()
  (root / "sysfs").mkdir()
  active = {"model_id": "owned-generic", "filename": "model.pkl", "size": model.stat().st_size, "sha256": MODEL_SHA, "url": "https://owned.invalid/model.pkl"}
  (cache / "state.json").write_text(json.dumps({"active": active, "previous": None}))
  os.link(model, cache / f"model-{MODEL_SHA[:16]}.pkl")
  if scenario != "missing-compiled":
    os.link(model, target / "model.pkl")
    os.link(archive, target / "runtime.tar.gz")
    (target / "installed.json").write_text(json.dumps(marker))
    with tarfile.open(archive) as runtime:
      for name in ("examples/openpilot/compile_warp.py", "tinygrad/__init__.py"):
        entry = runtime.extractfile(name)
        assert entry is not None
        destination = target / marker["runtime_directory"] / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(entry.read())
  if scenario not in ("absent", "grace"):
    present(root / "sysfs")


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  for name in ("binary", "worker", "model", "runtime", "installed", "assets", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  parser.add_argument("--retry-error", action="append")
  parser.add_argument("--expect-differences", type=int, default=0)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  assert args.model.stat().st_size == 776634338 and digest(args.model) == MODEL_SHA
  assert args.runtime.stat().st_size == 2731184 and digest(args.runtime) == RUNTIME_SHA
  marker = json.loads(args.installed.read_text())
  marker["catalog_url"] = "https://owned.invalid/precompiled.json"
  marker["pickle"]["url"] = "https://owned.invalid/model.pkl"
  marker["runtime"] = {"sha256": RUNTIME_SHA, "size": 2731184, "url": "https://owned.invalid/precompiled-runtime.tar.gz"}
  marker["runtime_directory"] = f"runtime-{RUNTIME_SHA[:16]}"
  cases = [(name, None) for name in ("absent", "missing-compiled", "startup-failed", "normal", "grace", "load-error", "disconnect")]
  cases += [("retry", error) for error in args.retry_error or ("pcie link not up",)]
  rows = []
  for index, (scenario, error) in enumerate(cases):
    pair = args.evidence / f"{index:02d}-{scenario}"
    pair.mkdir()
    source_root, native_root = pair / "source", pair / "native"
    for root in (source_root, native_root):
      setup(root, args.model, args.runtime, marker, scenario)
    expected = source(source_root, scenario, error)
    assert expected["selected"] == (scenario in ("normal", "grace", "retry", "disconnect")), expected
    assert expected["initial"]["UsbGpuCompiled"] == (scenario != "missing-compiled"), expected
    env = os.environ.copy()
    lifecycle = native_root / "workers.jsonl"
    env["USBGPU_WORKER_TEST_LIFECYCLE_LOG"] = str(lifecycle)
    if scenario == "load-error":
      env["USBGPU_WORKER_TEST_LOAD_ERROR"] = "owned loader failure"
    if scenario == "disconnect":
      env["USBGPU_WORKER_TEST_DISCONNECT"] = "1"
    if scenario == "retry":
      env["USBGPU_WORKER_TEST_RETRY_FILE"] = str(native_root / "attempts")
      env["USBGPU_WORKER_TEST_RETRY_ERROR"] = error
    argv = [str(args.binary), str(args.worker), str(native_root), str(args.assets), scenario]
    result = subprocess.run(argv, text=True, capture_output=True, env=env)
    receipt = native_root / "comparison.json"
    actual = json.loads(receipt.read_text()) if receipt.is_file() else None
    workers = [json.loads(line) for line in lifecycle.read_text().splitlines()] if lifecycle.exists() else []
    reaped = all(not Path(f"/proc/{worker['pid']}").exists() and not Path(worker["shared"]).exists() for worker in workers)
    equal = (
      actual is not None
      and result.returncode == 0
      and actual["passed"]
      and reaped
      and all(actual[name] == expected[name] for name in ("initial", "final", "selected"))
    )
    if scenario == "retry":
      equal = equal and actual.get("attempts") == expected["attempts"]
    rows.append(
      {
        "scenario": scenario,
        "retry_error": error,
        "source": expected,
        "native": actual,
        "argv": argv,
        "exit": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "workers": workers,
        "workers_reaped_and_shared_removed": reaped,
        "equal": equal,
      }
    )
    report = {
      "rows": rows,
      "differences": sum(not row["equal"] for row in rows),
      "binary_sha256": digest(args.binary),
      "worker_sha256": digest(args.worker),
      "runtime_sha256": RUNTIME_SHA,
      "model_sha256": MODEL_SHA,
      "source_exception_controls": exception_controls(),
      "scope": (
        "Original modeld startup statements and actual native selection/binding/parsed prediction. "
        + "Genuine PKL/companions, deterministic owned protocol worker; no GPU execution or full-daemon acceptance."
      ),
    }
    (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  assert report["differences"] == args.expect_differences, report["differences"]
  print(json.dumps({"cases": len(rows), "differences": report["differences"]}))


if __name__ == "__main__":
  main()
