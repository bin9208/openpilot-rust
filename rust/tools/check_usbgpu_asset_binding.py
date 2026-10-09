from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess

from usbgpu_boot_fixture import native, setup


def main() -> None:
  parser = argparse.ArgumentParser(description="Observe validated native assets through actual client and boot child launches")
  for name in ("binary", "binding", "worker", "protocol-worker", "metadata", "installed", "package", "usb-library", "evidence"):
    parser.add_argument(f"--{name}", type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True)
  manifest_sha = sha256((args.package / "manifest.json").read_bytes()).hexdigest()
  binaries = {name: sha256(getattr(args, name).read_bytes()).hexdigest() for name in ("binary", "binding", "worker", "protocol_worker")}
  rows = []
  for change in (False, True):
    fixture = setup(args.evidence / ("changed-manifest" if change else "explicit-child"), args.installed, args.protocol_worker, args.metadata)
    package = args.package
    if change:
      package = fixture.root / "package"
      shutil.copytree(args.package, package)
    log = fixture.root / "usb.log"
    argv = [str(args.binding), str(fixture.installed), str(package), str(args.worker)]
    if change:
      argv.append("--change-manifest")
    env = dict(
      os.environ,
      USBGPU_ASSETS_ROOT=str(fixture.root / "wrong-ambient-root"),
      USBGPU_ASSETS_MANIFEST_SHA256="0" * 64,
      USB_FIXTURE_MODE="open_error",
      USB_FIXTURE_LOG=str(log),
      LD_LIBRARY_PATH=str(args.usb_library.parent),
    )
    result = subprocess.run(argv, env=env, text=True, capture_output=True, check=False, timeout=45)
    events = log.read_text().splitlines() if log.exists() else []
    row = {
      "name": "changed-manifest" if change else "explicit-child",
      "argv": argv,
      "exit": result.returncode,
      "stdout": result.stdout,
      "stderr": result.stderr,
      "usb_events": events,
      "ambient_root": env["USBGPU_ASSETS_ROOT"],
    }
    rows.append(row)
    (args.evidence / "result.json").write_text(json.dumps({"rows": rows, "binaries": binaries}, indent=2) + "\n")
    assert result.returncode == 0, row
    response = json.loads(result.stdout)
    assert response["root"] == str(package.resolve()) and response["manifest_sha256"] == manifest_sha, row
    assert response["error"] == (
      "native model assets: native asset manifest differs from validated binding" if change else "libusb_open: fixture USB failure"
    ), row
    assert events == ([] if change else ["init", "list freed", "exit clean"]), row
  fixture = setup(args.evidence / "boot-cache", args.installed, args.protocol_worker, args.metadata)
  package = fixture.root / "package"
  shutil.copytree(args.package, package)
  bindings = fixture.root / "child-bindings.txt"
  original = fixture.worker.read_text()
  fixture.worker.write_text(
    original.replace("exec ", f'printf "%s\\n%s\\n" "$USBGPU_ASSETS_ROOT" "$USBGPU_ASSETS_MANIFEST_SHA256" >> {shlex.quote(str(bindings))}\nexec ', 1)
  )
  saved = {key: os.environ.get(key) for key in ("USBGPU_ASSETS_ROOT", "USBGPU_ASSETS_MANIFEST_SHA256")}
  try:
    os.environ["USBGPU_ASSETS_ROOT"] = str(fixture.root / "wrong-ambient-root")
    os.environ["USBGPU_ASSETS_MANIFEST_SHA256"] = "0" * 64
    first = native(args.binary, package, fixture)
    first_key = json.loads((fixture.installed.parent / "boot_validation.json").read_text())["key"]
    second = native(args.binary, package, fixture)
    with (package / "manifest.json").open("ab") as file:
      file.write(b"\n")
    next_sha = sha256((package / "manifest.json").read_bytes()).hexdigest()
    third = native(args.binary, package, fixture)
    next_key = json.loads((fixture.installed.parent / "boot_validation.json").read_text())["key"]
  finally:
    for key, value in saved.items():
      if value is None:
        os.environ.pop(key, None)
      else:
        os.environ[key] = value
  lines = bindings.read_text().splitlines()
  row = {
    "name": "boot-cache",
    "first": first,
    "second": second,
    "changed": third,
    "first_key": first_key,
    "changed_key": next_key,
    "bindings": lines,
    "manifest_sha256": manifest_sha,
    "changed_manifest_sha256": next_sha,
  }
  rows.append(row)
  (args.evidence / "result.json").write_text(json.dumps({"rows": rows, "binaries": binaries}, indent=2) + "\n")
  assert all(result["exit"] == 0 and result["prepared"] for result in (first, second, third)), row
  assert first["new_worker_inputs"] and not second["new_worker_inputs"] and third["new_worker_inputs"], row
  assert first_key != next_key and next_sha != manifest_sha, row
  assert lines == [str(package.resolve()), manifest_sha, str(package.resolve()), next_sha], row
  assert all(sha256(getattr(args, name).read_bytes()).hexdigest() == digest for name, digest in binaries.items())
  print(json.dumps({"cases": len(rows), "child_bindings": 2, "cache_invalidated": True}))


if __name__ == "__main__":
  main()
