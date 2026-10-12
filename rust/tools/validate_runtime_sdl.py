from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

from validate_runtime_ffmpeg import elf

WHEEL_SHA256 = "c47a6938de93fa610accd4969e638c2aebcb29b2fca518a84c3a39d91ab47116"
TTF_SHA256 = "78cdad51f3cc3ada6932b1bb6e914b33798ab970a1e817763f22ddbfd97d0c57"


def check_digest(path, expected):
  with path.open("rb") as stream:
    if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
      raise RuntimeError(f"source digest mismatch: {path}")


def main():
  cli = argparse.ArgumentParser()
  for name in ["wheel", "ttf-source", "cc", "sdk", "qemu", "loader", "agnos-libraries", "output"]:
    cli.add_argument("--" + name, type=Path, required=True)
  cli.add_argument("--source-python", type=Path)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  if shutil.disk_usage(args.output).free < 26 * 1024**3:
    raise RuntimeError("SDL provider preparation requires 25 GiB reserve plus 1 GiB growth")
  check_digest(args.wheel, WHEEL_SHA256)
  check_digest(args.ttf_source, TTF_SHA256)
  library = args.output / "lib"
  headers = args.output / "include"
  library.mkdir(exist_ok=True)
  headers.mkdir(exist_ok=True)
  with tarfile.open(args.ttf_source) as archive:
    member = archive.extractfile("SDL2_ttf-2.20.1/SDL_ttf.h")
    if member is None:
      raise RuntimeError("pinned SDL_ttf header absent")
    (headers / "SDL_ttf.h").write_bytes(member.read())
  with zipfile.ZipFile(args.wheel) as wheel:
    names = wheel.namelist()
    ttf_name = next(name for name in names if name.startswith("pygame.libs/libSDL2_ttf-"))
    pending = [ttf_name]
    extracted = set()
    while pending:
      name = pending.pop()
      if name in extracted:
        continue
      extracted.add(name)
      path = library / Path(name).name
      path.write_bytes(wheel.read(name))
      pending.extend("pygame.libs/" + dep for dep in elf(path)["needed"] if "pygame.libs/" + dep in names)
    font = args.output / "freesansbold.ttf"
    font.write_bytes(wheel.read("pygame/freesansbold.ttf"))
  sdl = next(library.glob("libSDL2-2-*"))
  ttf = library / Path(ttf_name).name
  probe = args.output / "sdl-provider.elf"
  command = [str(args.cc), "-std=c11", "-Wall", "-Wextra", "-Werror", "-I" + str(args.sdk / "usr/include/SDL2"),
             "-I" + str(args.sdk / "usr/include/aarch64-linux-gnu"), "-I" + str(headers),
             str(Path(__file__).parent / "fixtures/runtime_sdl_provider.c"), "-ldl", "-o", str(probe)]
  if shutil.disk_usage(args.output).free < 26 * 1024**3:
    raise RuntimeError("SDL probe compile would violate disk reserve")
  built = subprocess.run(command, capture_output=True, text=True, check=False)
  (args.output / "build.log").write_text(built.stdout + built.stderr)
  built.check_returncode()
  libraries = [library, args.agnos_libraries, args.sdk / "usr/lib/aarch64-linux-gnu"]
  paths, objects, providers = [probe, ttf, args.loader], {}, {}
  while paths:
    path = paths.pop().resolve()
    if path in objects:
      continue
    objects[path] = elf(path)
    for needed in objects[path]["needed"]:
      found = next((folder / needed for folder in libraries if (folder / needed).is_file()), None)
      if found is None:
        raise RuntimeError(f"missing target provider {needed}")
      providers[needed] = found.resolve()
      paths.append(found)
  exports = set().union(*(obj["exports"] for obj in objects.values()))
  report = {"wheel_sha256": WHEEL_SHA256, "ttf_source_sha256": TTF_SHA256, "build_command": command, "objects": []}
  for path, obj in objects.items():
    missing_versions = {provider: sorted(set(versions) - set(objects[providers[provider]]["definitions"]))
                        for provider, versions in obj["needs"].items()}
    missing_versions = {key: value for key, value in missing_versions.items() if value}
    report["objects"].append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "needed": obj["needed"],
                              "missing_versions": missing_versions, "missing_strong_symbols": sorted(obj["imports"] - exports)})
  report["abi_compatible"] = all(not obj["missing_versions"] and not obj["missing_strong_symbols"] for obj in report["objects"])
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  if not report["abi_compatible"]:
    raise RuntimeError("SDL target ABI closure failed")
  command = [str(args.qemu), str(args.loader), "--library-path", ":".join(map(str, libraries)), str(probe), str(sdl), str(ttf), str(font)]
  run = subprocess.run(command, env=os.environ | {"SDL_VIDEODRIVER": "dummy"}, capture_output=True, timeout=30, check=False)
  (args.output / "pixels.raw").write_bytes(run.stdout)
  (args.output / "probe.stderr").write_bytes(run.stderr)
  report["run"] = {"command": command, "exit_code": run.returncode, "details": run.stderr.decode(errors="replace"),
                   "pixel_bytes": len(run.stdout), "pixel_sha256": hashlib.sha256(run.stdout).hexdigest()}
  report["passed"] = run.returncode == 0 and len(run.stdout) > 0
  report["scope"] = "native SDL2/SDL_ttf providers from pinned aarch64 wheel, no Python runtime files; AGNOS loader, QEMU dummy window and text rasterization"
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  run.check_returncode()
  if not report["passed"]:
    raise RuntimeError("SDL rendered no pixels")
  if args.source_python:
    source_code = """import pygame, sys, json
pygame.font.init()
sys.stderr.write(json.dumps({'pygame': pygame.version.ver, 'sdl': pygame.get_sdl_version(), 'ttf': pygame.font.get_sdl_ttf_version()}))
font = pygame.font.Font(sys.argv[1], 18)
image = font.render('Speed 42 km/h', True, (255, 255, 255))
sys.stdout.buffer.write(pygame.image.tobytes(image, 'RGBA'))
pygame.font.quit()
"""
    expected = subprocess.run([str(args.source_python), "-c", source_code, str(font)], capture_output=True,
                              env=os.environ | {"PYGAME_HIDE_SUPPORT_PROMPT": "1"}, timeout=30, check=True)
    (args.output / "source-pixels.raw").write_bytes(expected.stdout)
    report["source_pixel_comparison"] = {"bytes": len(expected.stdout), "equal": expected.stdout == run.stdout,
                                         "sha256": hashlib.sha256(expected.stdout).hexdigest(), "versions": json.loads(expected.stderr)}
    report["passed"] = report["passed"] and report["source_pixel_comparison"]["equal"]
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report["passed"]:
      raise RuntimeError("AArch64 SDL_ttf pixels differ from original pygame font rendering")
  print(json.dumps({"objects": len(objects), "passed": True, "details": report["run"]["details"]}))


if __name__ == "__main__":
  main()
