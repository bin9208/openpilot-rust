"""Original artifact failure persistence against the native receipt boundary."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from pytest import MonkeyPatch

from check_usbgpu_provision_contract import invoke


def main() -> None:
  from openpilot.selfdrive.modeld import precompiled_model

  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--evidence", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  details = [
    "PCIe Link Not Up",
    "pcie power off failed",
    "pcie power on failed",
    "USB bridge reset failed",
    "read(0xb450: value mismatch",
    "f0 out failed: -1",
    "libusb_open: no such device",
    "AMD:0 does not exist",
    "precompiled eGPU worker timed out",
    "precompiled eGPU worker exited (1)",
    "fatal model output",
    "나" * 17000,
    "typed timeout",
    "typed broken pipe",
  ]
  rows = []
  model = {"model_id": "owned", "filename": "model.pkl", "size": 1, "sha256": "a" * 64, "url": "https://owned.invalid/model.pkl"}
  typed = {"typed timeout": (TimeoutError("typed timeout"), "timeout"), "typed broken pipe": (BrokenPipeError("typed broken pipe"), "broken_pipe")}
  for index, detail in enumerate(details):
    error, kind = typed.get(detail, (detail, "detail"))
    source, native = args.evidence / str(index) / "source", args.evidence / str(index) / "native"
    for directory in (source, native):
      directory.mkdir(parents=True)
      (directory / "installed.json").write_text(json.dumps({"pickle": {"sha256": model["sha256"]}}))
      (directory / "boot_validation.json").write_text('{"key":"old"}')
      if index == 0:
        (directory / "rejected").write_text("existing-rejection")
    with MonkeyPatch.context() as monkey:
      monkey.setattr(precompiled_model.time, "time", lambda: 1234.5)
      rejected = precompiled_model.record_failure(source / "model.pkl", error, "load")
    actual = invoke(
      args.binary, {"action": "failure", "cache": str(native), "model": model, "value": {"detail": detail, "kind": kind, "phase": "load", "wall_time": 1234.5}}
    )
    original_files = {path.name: path.read_text() for path in source.iterdir()}
    native_files = {path.name: path.read_text() for path in native.iterdir()}
    original_files["last_failure.json"] = json.loads(original_files["last_failure.json"])
    native_files["last_failure.json"] = json.loads(native_files["last_failure.json"])
    rows.append(
      {
        "name": index,
        "source_rejected": rejected,
        "native": actual,
        "source_files": original_files,
        "native_files": native_files,
        "equal": actual.get("result") == {"rejected": rejected} and original_files == native_files,
      }
    )
  result = {"rows": rows, "differences": sum(not row["equal"] for row in rows)}
  (args.evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps({"cases": len(rows), "differences": result["differences"]}))
  assert result["differences"] == 0


if __name__ == "__main__":
  main()
