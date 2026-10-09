from __future__ import annotations

import argparse
from enum import StrEnum
from hashlib import sha256
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from typing import assert_never

from usbgpu_boot_fixture import Json, setup


class Case(StrEnum):
  BIG = "ready-big"
  SMALL = "ready-small"
  MISSING = "missing-root"
  CORRUPT = "corrupt-package"
  IDENTITY = "wrong-manifest-identity"
  CHECKPOINT = "wrong-checkpoint"
  MODEL = "corrupt-pickle"
  LEGACY = "unverified-adjacent-ignored"
  DEFAULT = "default-missing-no-adjacent-fallback"


def main() -> None:
  parser = argparse.ArgumentParser(description="Verify real worker artifact preparation before GPU initialization")
  for name in ("binary", "metadata", "installed", "package", "usb-library", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  model_size = args.installed.stat().st_size
  manifest_sha = sha256((args.package / "manifest.json").read_bytes()).hexdigest()
  metadata = json.loads(args.metadata.read_text())
  declared = json.loads((args.package / "manifest.json").read_text())
  descriptor = next(entry for entry in declared["files"] if entry["path"].endswith("/model.hcq.json"))["sha256"]
  assert not args.binary.with_name("usbgpu-assets").exists(), "default target package must be absent for the fallback control"
  rows: list[dict[str, Json]] = []
  for case in Case:
    fixture = setup(args.evidence / case, args.installed, args.binary, args.metadata)
    package, expected = args.package, manifest_sha
    error = None
    camera = [1344, 760] if case == Case.SMALL else [1928, 1208]
    structural = case in (Case.BIG, Case.SMALL, Case.LEGACY)
    match case:
      case Case.MISSING:
        package = fixture.root / "absent"
        error = "No such file or directory (os error 2)"
      case Case.CORRUPT:
        package = fixture.root / "package"
        shutil.copytree(args.package, package)
        (package / "warp-gfx1200-1928x1208.json").write_bytes(b"corrupt")
        error = "native asset checksum mismatch: warp-gfx1200-1928x1208.json"
      case Case.IDENTITY:
        expected = "0" * 64
        error = "native asset manifest differs from validated binding"
      case Case.CHECKPOINT:
        marker = fixture.installed.parent / "installed.json"
        value = json.loads(marker.read_text())
        value["model_checkpoint"] = "wrong"
        marker.write_text(json.dumps(value))
        error = "precompiled checkpoint mismatch"
      case Case.MODEL:
        fixture.installed.unlink()
        with fixture.installed.open("wb") as file:
          file.truncate(model_size)
        error = "precompiled PKL checksum mismatch"
      case Case.LEGACY | Case.DEFAULT:
        fixture.installed.with_suffix(".hcq.json").write_bytes(b"unverified descriptor")
        fixture.installed.with_suffix(".hcq-meta.json").write_bytes(b"unverified metadata")
        (fixture.installed.parent / "firmware").mkdir()
      case Case.BIG | Case.SMALL:
        pass
      case unreachable:
        assert_never(unreachable)
    log = fixture.root / "usb.log"
    env = dict(
      os.environ,
      USBGPU_ASSETS_ROOT=str(package),
      USBGPU_ASSETS_MANIFEST_SHA256=expected,
      LD_LIBRARY_PATH=str(args.usb_library.parent),
      USB_FIXTURE_MODE="open_error",
      USB_FIXTURE_LOG=str(log),
    )
    if case == Case.DEFAULT:
      env.pop("USBGPU_ASSETS_ROOT")
      env.pop("USBGPU_ASSETS_MANIFEST_SHA256")
      error = "No such file or directory (os error 2)"
    shared = fixture.root / "shared"
    shared.touch()
    argv = (
      [str(args.binary), "--check-artifacts", str(fixture.installed), *map(str, camera)]
      if structural
      else [str(args.binary), str(fixture.installed), str(shared), *map(str, camera)]
    )
    before = sha256(args.binary.read_bytes()).hexdigest()
    started = time.monotonic()
    result = subprocess.run(argv, env=env, text=True, capture_output=True, check=False, timeout=40)
    events = log.read_text().splitlines() if log.exists() else []
    row = {
      "name": case,
      "argv": argv,
      "env": {name: env.get(name) for name in ("USBGPU_ASSETS_ROOT", "USBGPU_ASSETS_MANIFEST_SHA256", "LD_LIBRARY_PATH", "USB_FIXTURE_LOG")},
      "binary_sha256": before,
      "exit": result.returncode,
      "stdout": result.stdout,
      "stderr": result.stderr,
      "expected_error": error,
      "usb_events": events,
      "seconds": time.monotonic() - started,
    }
    rows.append(row)
    (args.evidence / "result.json").write_text(json.dumps({"rows": rows, "scope": "real worker artifact preparation; no GPU execution"}, indent=2) + "\n")
    assert sha256(args.binary.read_bytes()).hexdigest() == before
    assert events == [], row
    response = json.loads(result.stdout) if structural else json.loads(result.stdout.removeprefix("ERROR "))
    if structural:
      assert result.returncode == 0
      assert response["assets"] == str(package.resolve()) and response["manifest_sha256"] == manifest_sha
      assert response["descriptor_sha256"] == descriptor
      assert response["checkpoint"] == metadata["checkpoint"] and response["output_count"] == metadata["output_count"]
      assert response["camera"] == camera
    else:
      assert result.returncode == 1 and response == f"native model assets: {error}", row
      assert result.stderr == f"native model assets: {error}\n", row
  print(json.dumps({"cases": len(rows), "usb_calls": 0}))


if __name__ == "__main__":
  main()
