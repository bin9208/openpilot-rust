from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from runtime_elf_audit import audit


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--sdk", type=Path, required=True)
  cli.add_argument("--agnos", type=Path, required=True)
  cli.add_argument("--qemu", type=Path, required=True)
  cli.add_argument("--probe", type=Path, required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  libraries = [args.sdk / "usr/lib/aarch64-linux-gnu", args.agnos]
  specifications = [
    ("libportaudio.so.2", ["portaudio/src/native.rs"]),
    ("libusb-1.0.so.0", ["panda-usb/src/api.rs", "usbgpu/src/native_usb/api.rs"]),
    ("librtmp.so.1", ["carrot-server/src/youtube_live/rtmp_api.rs"]),
  ]
  roots = [libraries[0] / name for name, _ in specifications]
  report = audit(roots, libraries)
  report["runs"] = []
  for (name, sources), library in zip(specifications, roots, strict=True):
    symbols = set()
    hashes = {}
    for source in sources:
      path = args.root / "rust/crates" / source
      symbols.update(re.findall(r'b"([A-Za-z][A-Za-z0-9_]*)\\0"', path.read_text()))
      hashes[source] = hashlib.sha256(path.read_bytes()).hexdigest()
    if not symbols:
      raise ValueError(f"no current source symbols for {name}")
    command = [str(args.qemu), str(args.agnos / "ld-linux-aarch64.so.1"), "--library-path",
               ":".join(map(str, libraries)), str(args.probe), str(library), "symbols", *sorted(symbols)]
    run = subprocess.run(command, capture_output=True, text=True, timeout=30, check=False)
    report["runs"].append({"name": name, "sources": hashes, "symbols": sorted(symbols), "command": command,
                           "exit_code": run.returncode, "stdout": run.stdout, "stderr": run.stderr})
  report["passed"] = report["compatible"] and all(run["exit_code"] == 0 for run in report["runs"])
  report["execution_scope"] = "AGNOS native provider dlopen/dlsym with QEMU; no USB, audio stream or network initialization"
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"passed": report["passed"], "objects": len(report["objects"]),
                    "symbols": {run["name"]: len(run["symbols"]) for run in report["runs"]}}))
  if not report["passed"]:
    raise RuntimeError("native dynamic provider validation failed")


if __name__ == "__main__":
  main()
