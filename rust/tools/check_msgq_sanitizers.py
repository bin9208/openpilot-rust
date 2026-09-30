#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def check(target: Path) -> None:
  environment = os.environ | {"CARGO_TARGET_DIR": str(target), "CXXFLAGS": "-fsanitize=address,undefined -fno-omit-frame-pointer",
                              "RUSTFLAGS": "-C link-arg=-lasan -C link-arg=-lubsan"}
  build = subprocess.run(["cargo", "test", "-p", "openpilot-msgq", "--test", "transport", "--test", "vision", "--test", "queued",
                          "--no-run", "--locked", "--message-format=json"],
                         cwd=ROOT, env=environment, stdout=subprocess.PIPE, text=True, check=True)
  binaries: list[str] = []
  for line in build.stdout.splitlines():
    message = json.loads(line)
    if message.get("reason") == "compiler-artifact" and message.get("executable"):
      binaries.append(message["executable"])
  if len(binaries) != 3:
    raise RuntimeError(f"expected transport, queued and VisionIPC test executables, got {binaries}")
  runtime = subprocess.check_output(["g++", "-print-file-name=libasan.so"], text=True).strip()
  environment |= {"LD_PRELOAD": runtime, "ASAN_OPTIONS": "detect_leaks=1", "UBSAN_OPTIONS": "halt_on_error=1"}
  for binary in binaries:
    subprocess.run([binary, "--nocapture"], env=environment, check=True, timeout=60)


if __name__ == "__main__":
  parser = argparse.ArgumentParser(description="Instrument the native msgq boundary and peer with ASan and UBSan")
  parser.add_argument("--target-dir", required=True, type=Path)
  check(parser.parse_args().target_dir.resolve())
