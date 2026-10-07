#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess


def digest(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command: list[str], log: Path, env: dict[str, str]) -> None:
  with log.open("wb") as output:
    subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, env=env, check=True, timeout=120)


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare unchanged source FFmpeg/JPEG method bodies with Rust codec output")
  parser.add_argument("--rust", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--jpeg-build", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  arguments = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = arguments.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  source = root / "openpilot/system/loggerd/encoder"
  fixture = Path(__file__).with_name("encoder_oracle")
  pieces = [fixture.joinpath("adapter.h").read_text()]
  receipts: dict[str, str] = {}
  for name, marker in [
    ("ffmpeg_encoder.h", "class FfmpegEncoder"),
    ("jpeg_encoder.h", "class JpegEncoder"),
    ("ffmpeg_encoder.cc", "FfmpegEncoder::FfmpegEncoder"),
    ("jpeg_encoder.cc", "JpegEncoder::JpegEncoder"),
  ]:
    path = source / name
    content = path.read_text()
    pieces.append(content[content.index(marker) :])
    receipts[str(path.relative_to(root))] = digest(path)
  pieces.append(fixture.joinpath("main.cc").read_text())
  generated = output / "source-oracle.cc"
  generated.write_text("\n".join(pieces))
  env = dict(os.environ)
  native = arguments.native.resolve()
  library = native / "usr/lib/x86_64-linux-gnu"
  env["PKG_CONFIG_LIBDIR"] = str(library / "pkgconfig")
  env["PKG_CONFIG_SYSROOT_DIR"] = str(native)
  env["LD_LIBRARY_PATH"] = str(library) + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
  flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "libavcodec", "libavformat", "libavutil"], env=env, text=True))
  jpeg_source = root / "rust/crates/jpeg/native/vendor/src"
  jpeg_build = arguments.jpeg_build.resolve()
  command = [
    "g++",
    "-std=c++17",
    "-O1",
    "-g0",
    str(generated),
    "-I" + str(native / "usr/include"),
    "-I" + str(jpeg_source),
    "-I" + str(jpeg_build),
    *flags,
    str(library / "libyuv.a"),
    str(jpeg_build / "libjpeg.a"),
    "-o",
    str(output / "source-oracle"),
  ]
  free = shutil.disk_usage(output).free
  guard = {"free": free, "growth": 128 * 1024**2, "floor": 25 * 1024**3}
  (output / "space.json").write_text(json.dumps(guard, indent=2) + "\n")
  if free < guard["floor"] + guard["growth"]:
    raise RuntimeError(f"insufficient disk headroom: {guard}")
  (output / "build-command.json").write_text(json.dumps(command, indent=2) + "\n")
  run(command, output / "build.log", env)
  cases = [
    ("lossless", 128, 80, 128, 80, 160, 8),
    ("lossless", 256, 160, 128, 80, 288, 8),
    ("h264", 256, 160, 128, 80, 288, 80),
    ("jpeg", 1928, 1208, 482, 302, 2048, 3),
    ("jpeg", 1280, 720, 320, 180, 1344, 3),
  ]
  results: list[dict[str, object]] = []
  for index, case in enumerate(cases):
    name = f"case-{index}-{case[0]}"
    expected = output / f"{name}-source"
    actual = output / f"{name}-rust"
    values = [str(value) for value in case]
    run([str(output / "source-oracle"), str(expected), *values], output / f"{name}-source.log", env)
    run([str(arguments.rust.resolve()), str(actual), *values], output / f"{name}-rust.log", env)
    expected_trace = (expected / "trace.tsv").read_text()
    actual_trace = (actual / "trace.tsv").read_text()
    if expected_trace != actual_trace:
      raise AssertionError(f"{name}: packet metadata or encode returns differ")
    files = sorted(expected.glob("packet-*.bin"))
    if not files or len(files) != len(list(actual.glob("packet-*.bin"))):
      raise AssertionError(f"{name}: missing or unexpected packets")
    packet_hashes = {}
    for packet in files:
      expected_hash = digest(packet)
      if expected_hash != digest(actual / packet.name):
        raise AssertionError(f"{name}/{packet.name}: compressed bytes differ")
      packet_hashes[packet.name] = expected_hash
    results.append({"case": case, "packets": len(files), "trace_sha256": digest(expected / "trace.tsv"), "packet_sha256": packet_hashes})
  receipt = {
    "source_sha256": receipts,
    "rust_binary_sha256": digest(arguments.rust.resolve()),
    "oracle_sha256": digest(output / "source-oracle"),
    "generated_sha256": digest(generated),
    "native_sha256": {
      str(path): digest(path)
      for path in [library / "libavcodec.so.60.31.102", library / "libavutil.so.58.29.100", library / "libyuv.a", jpeg_build / "libjpeg.a"]
    },
    "results": results,
    "status": "PASS",
    "scope": "host source-method codec bytes and metadata; publication transport and hardware are separate",
  }
  (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(json.dumps({"status": "PASS", "cases": len(results), "packets": sum(int(row["packets"]) for row in results)}))


if __name__ == "__main__":
  main()
