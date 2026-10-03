#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python3 rust/tools/check_encoder_target.py --help
# Supply explicit frozen ARM ELF, JPEG, cross-compiler, package and AGNOS paths.
# No downloads, Cargo builds, physical devices or host codec fallback are used.
# ──────────────────
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import struct
import subprocess
import time
from typing import Final


ROOT: Final = Path(__file__).resolve().parents[2]
CASES: Final = (
  ("lossless", 128, 80, 128, 80, 160, 8),
  ("lossless", 256, 160, 128, 80, 288, 8),
  ("h264", 256, 160, 128, 80, 288, 80),
  ("jpeg", 1928, 1208, 482, 302, 2048, 3),
  ("jpeg", 1280, 720, 320, 180, 1344, 3),
)
PACKET_COUNTS: Final = (16, 16, 68, 6, 6)
LIBRARIES: Final = ("avformat", "avcodec", "avutil", "swresample", "x264", "z", "va", "va-drm", "drm")
VERSION_LINE: Final = "encoder-target b08d7969 4002660 3999588 3876708 1922"


@dataclass(frozen=True, slots=True)
class TargetFailure(RuntimeError):
  detail: str

  def __str__(self) -> str:
    return self.detail


@dataclass(frozen=True, slots=True)
class Options:
  rust: Path
  rust_sha256: str
  jpeg_build: Path
  jpeg_archive: Path
  jpeg_sha256: str
  ffmpeg: Path
  ffmpeg_receipt: Path
  libyuv: Path
  libyuv_receipt: Path
  compiler: Path
  compiler_host_libs: Path
  qemu: Path
  agnos_libs: Path
  output: Path
  pin_receipt: list[Path]


@dataclass(frozen=True, slots=True)
class FileRecord:
  path: str
  sha256: str
  bytes: int


def identify(path: Path) -> FileRecord:
  with path.open("rb") as stream:
    return FileRecord(str(path), hashlib.file_digest(stream, "sha256").hexdigest(), path.stat().st_size)


def require(condition: bool, detail: str) -> None:
  if not condition:
    raise TargetFailure(detail)


def arm_elf(path: Path) -> None:
  with path.open("rb") as stream:
    header = stream.read(20)
  require(header[:6] == b"\x7fELF\x02\x01" and len(header) == 20 and struct.unpack_from("<H", header, 18)[0] == 183,
          f"not a little-endian AArch64 ELF: {path}")


def disk_guard(directory: Path, growth: int) -> None:
  free = shutil.disk_usage(directory).free
  record = {"free_bytes": free, "growth_bytes": growth, "floor_bytes": 25 * 1024**3, "monotonic_ns": time.monotonic_ns()}
  with (directory / "disk-guards.jsonl").open("a") as stream:
    stream.write(json.dumps(record) + "\n")
  require(free >= 25 * 1024**3 + growth, "insufficient disk space; recover at least 35 GiB before resuming")


def no_core() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def command(directory: Path, argv: list[str], environment: dict[str, str]) -> bytes:
  directory.mkdir()
  disk_guard(directory, 256 * 1024**2)
  record = {"argv": argv, "cwd": str(ROOT), "started_monotonic_ns": time.monotonic_ns(), "returncode": None,
            "timeout_seconds": 300, "status": "RUNNING", "environment_overrides": {
              key: environment.get(key) for key in ("LC_ALL", "DEBUG_ENCODER", "LD_LIBRARY_PATH", "LD_PRELOAD")}}
  path = directory / "command.json"
  path.write_text(json.dumps(record, indent=2) + "\n")
  try:
    with (directory / "stdout.bin").open("wb") as stdout, (directory / "stderr.bin").open("wb") as stderr:
      result = subprocess.run(argv, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, timeout=300, check=False, preexec_fn=no_core)
    record.update(returncode=result.returncode, status="PASS" if result.returncode == 0 else "FAIL")
  except subprocess.TimeoutExpired:
    record["status"] = "TIMEOUT"
    raise
  except OSError as error:
    record.update(status="EXEC_ERROR", error=str(error))
    raise
  finally:
    record["ended_monotonic_ns"] = time.monotonic_ns()
    path.write_text(json.dumps(record, indent=2) + "\n")
  require(result.returncode == 0, f"command failed: {path}")
  return (directory / "stdout.bin").read_bytes()


def source_oracle(output: Path) -> list[FileRecord]:
  source = ROOT / "openpilot/system/loggerd/encoder"
  fixture = ROOT / "rust/tools/encoder_oracle"
  inputs = [fixture / "adapter.h", fixture / "main.cc"]
  pieces = [inputs[0].read_text()]
  for name, marker in (("ffmpeg_encoder.h", "class FfmpegEncoder"), ("jpeg_encoder.h", "class JpegEncoder"),
                       ("ffmpeg_encoder.cc", "FfmpegEncoder::FfmpegEncoder"), ("jpeg_encoder.cc", "JpegEncoder::JpegEncoder")):
    path = source / name
    text = path.read_text()
    pieces.append(text[text.index(marker):])
    inputs.append(path)
  pieces.append('''#include <libyuv/version.h>
static const bool version_recorded = [] {
  fprintf(stderr, "encoder-target %s %u %u %u %d\\n", av_version_info(), avcodec_version(),
          avformat_version(), avutil_version(), LIBYUV_VERSION);
  return true;
}();
''')
  pieces.append(inputs[1].read_text())
  generated = output / "source-oracle.cc"
  generated.write_text("\n".join(pieces))
  return [identify(path) for path in [*inputs, generated]]


def loaded_libraries(output: bytes, root: Path) -> list[FileRecord]:
  paths = {Path(path).resolve() for path in re.findall(r"(/[^\s]+)\s+\(", output.decode())}
  require(bool(paths), "AGNOS loader did not report resolved library paths")
  require(all(path.is_relative_to(root.resolve()) for path in paths), "target loader resolved a library outside the supplied AGNOS tree")
  return [identify(path) for path in sorted(paths)]


def verify_packages(options: Options, records: list[FileRecord]) -> None:
  ffmpeg = json.loads(options.ffmpeg_receipt.read_text())
  yuv = json.loads(options.libyuv_receipt.read_text())
  require(ffmpeg["wheel_sha256"] == "ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4", "FFmpeg release pin differs")
  require(yuv["sha256"] == "1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475", "libyuv release pin differs")
  ffmpeg_files = {row["path"]: row["sha256"] for row in ffmpeg["files"]}
  for record in records:
    path = Path(record.path)
    if path.is_relative_to(options.ffmpeg):
      relative = "ffmpeg-7.1.0.data/purelib/ffmpeg/install/" + str(path.relative_to(options.ffmpeg))
      require(ffmpeg_files.get(relative) == record.sha256, f"FFmpeg payload differs from pinned receipt: {path}")
    if path.is_relative_to(options.libyuv):
      relative = "extracted/libyuv-1922.0.data/purelib/libyuv/install/" + str(path.relative_to(options.libyuv))
      require(yuv["files"].get(relative) == record.sha256, f"libyuv payload differs from pinned receipt: {path}")


def compare(case: Path, expected_count: int) -> list[FileRecord]:
  source, native = case / "source", case / "rust"
  source_trace, native_trace = source / "trace.tsv", native / "trace.tsv"
  require(source_trace.read_bytes() == native_trace.read_bytes(), f"metadata/return values differ: {case}")
  expected = {path.name for path in source.glob("packet-*.bin")}
  actual = {path.name for path in native.glob("packet-*.bin")}
  require(expected == actual and len(expected) == expected_count, f"missing/extra packet files: {case}: {len(expected)}/{len(actual)}")
  records = [identify(source_trace), identify(native_trace)]
  for name in sorted(expected):
    left, right = source / name, native / name
    require(left.read_bytes() == right.read_bytes(), f"compressed bytes differ: {case}/{name}")
    records.extend((identify(left), identify(right)))
  return records


def run(options: Options) -> None:
  output = options.output
  output.mkdir(parents=True, exist_ok=False)
  status = "FAIL"
  records: list[FileRecord] = []
  completed: list[str] = []
  try:
    disk_guard(output, 512 * 1024**2)
    inputs = [options.rust, options.jpeg_archive, options.compiler, options.qemu,
              options.ffmpeg_receipt, options.libyuv_receipt, Path(__file__).resolve(), *options.pin_receipt]
    records.extend(identify(path) for path in inputs)
    require(records[0].sha256 == options.rust_sha256, "frozen Rust ELF hash differs")
    require(records[1].sha256 == options.jpeg_sha256, "frozen target JPEG archive hash differs")
    arm_elf(options.rust)
    loader = options.agnos_libs / "ld-linux-aarch64.so.1"
    arm_elf(loader)
    records.append(identify(loader))
    records.extend(source_oracle(output))
    archives = [options.ffmpeg / "lib" / f"lib{name}.a" for name in LIBRARIES]
    archives.extend((options.libyuv / "lib/libyuv.a", options.jpeg_archive))
    records.extend(identify(path) for path in archives)
    include = [options.ffmpeg / "include", options.libyuv / "include", ROOT / "rust/crates/jpeg/native/vendor/src", options.jpeg_build]
    for folder in include:
      require(folder.is_dir(), f"missing explicit include directory: {folder}")
      records.extend(identify(path) for path in sorted(folder.rglob("*.h")))
    verify_packages(options, records)
    environment = {key: value for key, value in os.environ.items() if key not in ("LD_PRELOAD", "LD_LIBRARY_PATH") and not key.startswith("QEMU_")}
    environment.update(LC_ALL="C", DEBUG_ENCODER="0")
    build_environment = dict(environment, LD_LIBRARY_PATH=str(options.compiler_host_libs))
    oracle = output / "source-oracle"
    argv = [str(options.compiler), "-std=c++17", "-O1", "-g0", "-pthread", str(output / "source-oracle.cc")]
    argv.extend(f"-I{path}" for path in include)
    argv.extend(["-Wl,--start-group", *map(str, archives), "-Wl,--end-group", "-lm", "-ldl", "-o", str(oracle)])
    command(output / "build", argv, build_environment)
    arm_elf(oracle)
    records.append(identify(oracle))
    runner = [str(options.qemu), str(loader), "--library-path", str(options.agnos_libs)]
    for label, binary in (("source", oracle), ("rust", options.rust)):
      listing = command(output / f"loader-{label}", [*runner, "--list", str(binary)], environment)
      records.extend(loaded_libraries(listing, options.agnos_libs))
    for number, (case, count) in enumerate(zip(CASES, PACKET_COUNTS, strict=True)):
      directory = output / f"case-{number}-{case[0]}"
      directory.mkdir()
      for label, binary in (("source", oracle), ("rust", options.rust)):
        command(directory / f"run-{label}", [*runner, str(binary), str(directory / label), *map(str, case)], environment)
      diagnostic = (directory / "run-source/stderr.bin").read_text()
      require(VERSION_LINE in diagnostic.splitlines(), f"pinned codec runtime versions differ: {directory}")
      records.extend(compare(directory, count))
      completed.append(directory.name)
    require(sum(PACKET_COUNTS) == 112 and len(completed) == 5, "incomplete required target matrix")
    require(identify(options.rust).sha256 == options.rust_sha256, "frozen Rust ELF changed during comparison")
    status = "PASS"
  except (TargetFailure, OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
    (output / "failure.json").write_text(json.dumps({"type": type(error).__name__, "error": str(error)}, indent=2) + "\n")
    raise
  finally:
    patterns = ("command.json", "stdout.bin", "stderr.bin", "disk-guards.jsonl", "trace.tsv", "packet-*.bin", "*.nv12", "failure.json")
    records.extend(identify(path) for pattern in patterns
                   for path in output.rglob(pattern))
    payload = {"status": status, "scope": "exact source-method ARM codec comparison under the supplied AGNOS loader; no device/performance claim",
               "options": asdict(options), "completed_cases": completed,
               "required_cases": CASES, "required_packet_counts": PACKET_COUNTS, "artifacts": [asdict(record) for record in records]}
    (output / "receipt.json").write_text(json.dumps(payload, indent=2, default=str) + "\n")
  print(json.dumps({"status": status, "cases": len(completed), "packets": sum(PACKET_COUNTS), "receipt": str(output / "receipt.json")}))


def main() -> None:
  parser = argparse.ArgumentParser(description="Exact pinned ARM source/Rust encoder codec QA through the AGNOS loader")
  for name in ("rust", "jpeg-build", "jpeg-archive", "ffmpeg", "ffmpeg-receipt", "libyuv", "libyuv-receipt",
               "compiler", "compiler-host-libs", "qemu", "agnos-libs", "output"):
    parser.add_argument("--" + name, type=lambda value: Path(value).resolve(), required=True)
  for name in ("rust-sha256", "jpeg-sha256"):
    parser.add_argument("--" + name, required=True)
  parser.add_argument("--pin-receipt", type=lambda value: Path(value).resolve(), action="append", required=True)
  run(Options(**vars(parser.parse_args())))


if __name__ == "__main__":
  main()
