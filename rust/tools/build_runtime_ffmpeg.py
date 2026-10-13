from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time

from build_loggerd_ffmpeg import SHA256, VERSION


def require_space(path: Path):
  available = shutil.disk_usage(path).free
  if available < 28 * 1024**3:
    raise RuntimeError("runtime FFmpeg requires 25 GiB reserve plus 3 GiB estimated growth")
  return available


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--archive", type=Path, required=True)
  cli.add_argument("--sdk", type=Path, required=True)
  cli.add_argument("--cc", type=Path, required=True)
  cli.add_argument("--cxx", type=Path, required=True)
  cli.add_argument("--binutils", type=Path, required=True)
  cli.add_argument("--binutils-libraries", type=Path)
  cli.add_argument("--jobs", type=int, default=2)
  args = cli.parse_args()
  if not 1 <= args.jobs <= 2:
    cli.error("jobs must be 1 or 2 for the bounded provider build")
  args.root = args.root.resolve()
  args.root.mkdir(parents=True, exist_ok=True)
  require_space(args.root)
  with args.archive.open("rb") as stream:
    if hashlib.file_digest(stream, "sha256").hexdigest() != SHA256:
      raise ValueError("FFmpeg archive differs from pinned source")
  source = args.root / f"ffmpeg-{VERSION}"
  if not source.exists():
    with tarfile.open(args.archive) as archive:
      archive.extractall(args.root, filter="data")
  sdk = args.sdk.resolve()
  libraries = sdk / "usr/lib/aarch64-linux-gnu"
  prefix = args.root / "install"
  work = args.root / "build"
  work.mkdir(exist_ok=True)
  env = os.environ | {"PKG_CONFIG_SYSROOT_DIR": str(sdk), "PKG_CONFIG_LIBDIR": str(libraries / "pkgconfig"), "PKG_CONFIG_PATH": ""}
  if args.binutils_libraries:
    env["LD_LIBRARY_PATH"] = str(args.binutils_libraries.resolve()) + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
  command = [
    str(source / "configure"),
    f"--prefix={prefix}",
    "--arch=aarch64",
    "--target-os=linux",
    "--enable-cross-compile",
    f"--cc={args.cc.resolve()}",
    f"--cxx={args.cxx.resolve()}",
    *[f"--{tool}={args.binutils.resolve() / ('aarch64-linux-gnu-' + tool)}" for tool in ["ar", "ranlib", "nm", "strip"]],
    "--enable-shared",
    "--disable-static",
    "--enable-pic",
    "--disable-debug",
    "--disable-doc",
    "--disable-ffplay",
    "--disable-autodetect",
    "--enable-gpl",
    "--enable-version3",
    "--enable-libx264",
    "--enable-libvpx",
    "--enable-openssl",
    "--enable-zlib",
    f"--extra-cflags=-I{sdk / 'usr/include'} -I{sdk / 'usr/include/aarch64-linux-gnu'}",
    f"--extra-ldflags=-L{libraries} -Wl,-rpath-link,{libraries}",
  ]
  results = {"source_version": VERSION, "source_sha256": SHA256, "license": "GPL-3.0-or-later", "sdk": str(sdk), "commands": []}
  record = args.root / "build-record.json"
  for index, argv in enumerate([command, ["make", f"-j{args.jobs}"], ["make", "install"]]):
    available = require_space(args.root)
    start = time.monotonic()
    log = args.root / f"build-{index}.log"
    with log.open("w") as output:
      result = subprocess.run(argv, cwd=work, env=env, stdout=output, stderr=subprocess.STDOUT, check=False)
    step = {"argv": argv, "exit_code": result.returncode, "seconds": time.monotonic() - start, "available_bytes": available, "log": str(log)}
    results["commands"].append(step)
    record.write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(step), flush=True)
    result.check_returncode()
  (prefix / "share/runtime-provider").mkdir(parents=True, exist_ok=True)
  for name in ["COPYING.GPLv3", "LICENSE.md"]:
    shutil.copyfile(source / name, prefix / "share/runtime-provider" / name)
  results["files"] = {str(path.relative_to(prefix)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(prefix.rglob("*")) if path.is_file()}
  record.write_text(json.dumps(results, indent=2) + "\n")
  print(json.dumps({"prefix": str(prefix), "files": len(results["files"])}))


if __name__ == "__main__":
  main()
