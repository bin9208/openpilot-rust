from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

from runtime_elf_audit import audit


def main():
  cli = argparse.ArgumentParser()
  for name in ["source-root", "cc", "qemu", "loader", "plugin", "sdk", "agnos-libraries", "output"]:
    cli.add_argument("--" + name, type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(args.output).free < 26 * 1024**3:
    raise RuntimeError("graphics provider proof requires 25 GiB reserve plus 1 GiB growth")
  probe = args.output / "library-probe.elf"
  command = [str(args.cc), "-std=c11", "-Wall", "-Wextra", "-Werror", str(Path(__file__).parent / "fixtures/runtime_library_probe.c"), "-ldl", "-o", str(probe)]
  built = subprocess.run(command, capture_output=True, text=True, check=False)
  (args.output / "build.log").write_text(built.stdout + built.stderr)
  built.check_returncode()
  source = args.source_root / "rust/crates/startup-ui/native"
  raylib = (source / "raylib_loader.cc").read_text()
  gles = (source / "gles.cc").read_text()
  egl = (source / "egl.cc").read_text()
  raylib_names = re.findall(r'"(\w+)"', raylib.split("const char *required[] =", 1)[1].split("};", 1)[0])
  raylib_names += re.findall(r"^FORWARD\([^,]+,\s*(\w+),", raylib, re.MULTILINE)
  gles_names = re.findall(r'LOAD\(\w+,\s*"(\w+)"\)', gles) + re.findall(r'gles.symbol<.*?>\("(\w+)"\)', egl)
  egl_names = re.findall(r'egl.symbol<.*?>\("(\w+)"\)', egl)
  extensions = re.findall(r'extension<.*?>\("(\w+)"', egl)
  libraries = [args.plugin.parent, args.agnos_libraries, args.sdk / "usr/lib/aarch64-linux-gnu"]
  egl_library = libraries[-1] / "libEGL.so"
  gles_library = libraries[-1] / "libGLESv2.so"
  report = audit([probe, args.plugin], libraries)
  report["build_command"] = command
  report["sources"] = {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in ["raylib_loader.cc", "gles.cc", "egl.cc"]}
  report["runs"] = []
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  if not report["compatible"]:
    raise RuntimeError("graphics ABI closure failed")
  groups = [("raylib", args.plugin, "contract", ["comma-deps-raylib-6.0.0.1.post103", *sorted(set(raylib_names))]),
            ("egl", egl_library, "symbols", sorted(set(egl_names))), ("gles", gles_library, "symbols", sorted(set(gles_names))),
            ("extensions", egl_library, "extensions", sorted(set(extensions)))]
  for name, library, mode, symbols in groups:
    command = [str(args.qemu), str(args.loader), "--library-path", ":".join(map(str, libraries)), str(probe), str(library), mode, *symbols]
    run = subprocess.run(command, capture_output=True, text=True, timeout=30, check=False)
    (args.output / f"{name}.stdout").write_text(run.stdout)
    (args.output / f"{name}.stderr").write_text(run.stderr)
    report["runs"].append({"name": name, "command": command, "exit_code": run.returncode, "output": run.stdout, "error": run.stderr})
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    run.check_returncode()
  report["passed"] = True
  report["execution_scope"] = "AArch64 AGNOS dlopen, raylib contract and current symbol/extension lookup; no display/GPU initialization"
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"objects": len(report["objects"]), "runs": [{"name": row["name"], "output": row["output"]} for row in report["runs"]]}))


if __name__ == "__main__":
  main()
