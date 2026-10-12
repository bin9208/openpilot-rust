"""Stage verified external media providers; retain AGNOS system dependencies."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil


def digest(path: Path) -> str:
  with path.open("rb") as source:
    return hashlib.file_digest(source, "sha256").hexdigest()


def copy(source: Path, destination: Path, expected: str) -> dict:
  if digest(source) != expected:
    raise ValueError(f"verified provider changed: {source}")
  if destination.exists() and digest(destination) != expected:
    raise ValueError(f"conflicting package artifact: {destination}")
  if destination.is_symlink():
    raise ValueError(f"unexpected package symlink: {destination}")
  destination.parent.mkdir(parents=True, exist_ok=True)
  if not destination.exists():
    shutil.copy2(source, destination)
  if digest(destination) != expected:
    raise ValueError(f"copied provider differs: {destination}")
  return {"source": str(source), "path": str(destination), "sha256": expected}


def alias(directory: Path, name: str, target: str) -> dict:
  destination = directory / name
  if destination.is_symlink():
    if destination.readlink() != Path(target):
      raise ValueError(f"conflicting provider alias: {destination}")
  elif destination.exists():
    raise ValueError(f"provider alias would replace a file: {destination}")
  else:
    destination.symlink_to(target)
  if not destination.resolve(strict=True).is_relative_to(directory.resolve()):
    raise ValueError(f"provider alias escapes package: {destination}")
  return {"path": str(destination), "target": target}


def main():
  cli = argparse.ArgumentParser(description=__doc__)
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--providers", type=Path, required=True)
  cli.add_argument("--ffmpeg-report", type=Path, required=True)
  cli.add_argument("--graphics-report", type=Path, required=True)
  args = cli.parse_args()
  root, providers = args.root.resolve(strict=True), args.providers.resolve(strict=True)
  if shutil.disk_usage(root).free < 26 * 1024**3:
    raise RuntimeError("media staging requires 25 GiB reserve plus 1 GiB growth")
  binaries = root / "rust/bin"
  lib = binaries / "lib"
  records, aliases = [], []
  reports = [json.loads(path.read_text()) for path in
             [args.ffmpeg_report, providers / "sdl-validated/report.json", args.graphics_report]]
  if any(not report.get("passed") for report in reports):
    raise ValueError("media provider validation has not passed")
  expected = {row["path"]: row["sha256"] for report in reports for row in report["objects"]}
  sources = list((providers / "sdl-validated/lib").iterdir())
  sources += [path for path in (providers / "ffmpeg/install/lib").glob("*.so.*") if not path.is_symlink()]
  sources += [providers / "media-sdk/usr/lib/aarch64-linux-gnu/libvpx.so.9.0.0",
              providers / "raylib/lib/libopenpilot-raylib.so"]
  for source in sources:
    records.append(copy(source, lib / source.name, expected[str(source.resolve(strict=True))]))
  for source in (providers / "ffmpeg/install/lib").glob("*.so*"):
    if source.is_symlink():
      aliases.append(alias(lib, source.name, source.resolve(strict=True).name))
  for name, target in [("libvpx.so.9", "libvpx.so.9.0.0"),
                       ("libSDL2-2.0.so.0", "libSDL2-2-6e5c51a6.0.so.0.2800.4"),
                       ("libSDL2_ttf-2.0.so.0", "libSDL2_ttf-2-e035a276.0.so.0.2000.1")]:
    aliases.append(alias(lib, name, target))
  for name in ["ffmpeg", "ffprobe"]:
    source = providers / "ffmpeg/install/bin" / name
    records.append(copy(source, binaries / name, expected[str(source)]))
  font = providers / "sdl-validated/freesansbold.ttf"
  records.append(copy(font, binaries / "cluster-replay/freesansbold.ttf", digest(font)))
  for source in (providers / "sdl-licenses").iterdir():
    records.append(copy(source, binaries / "licenses/sdl" / source.name, digest(source)))
  for source in (providers / "raylib/licenses").iterdir():
    records.append(copy(source, binaries / "licenses/raylib" / source.name, digest(source)))
  for source in (providers / "ffmpeg/ffmpeg-6.1.1").glob("COPYING.*"):
    records.append(copy(source, binaries / "licenses/ffmpeg" / source.name, digest(source)))
  for source, relative in [(providers / "raylib/include/raylib.h", "licenses/raylib/raylib.h"),
                           (providers / "raylib/plugin-build.json", "licenses/raylib/build.json"),
                           (providers / "ffmpeg/build-record.json", "licenses/ffmpeg/build.json"),
                           (providers / "vpx-package-metadata.json", "licenses/vpx/package.json"),
                           (providers / "vpx-sdk/usr/share/doc/libvpx9/copyright", "licenses/vpx/copyright")]:
    records.append(copy(source, binaries / relative, digest(source)))
  output = binaries / "media-provenance.json"
  output.write_text(json.dumps({"architecture": "aarch64", "ownership": "external native providers",
                               "files": records, "aliases": aliases,
                               "system_dependencies": "Use pinned AGNOS 19.8-carrot-bt1; system graphics, x264, OpenSSL, zlib and C runtime are not copied",
                               "sdl_wheel_sha256": reports[1]["wheel_sha256"]}, indent=2) + "\n")
  print(json.dumps({"files": len(records), "aliases": len(aliases), "provenance": str(output)}))


if __name__ == "__main__":
  main()
