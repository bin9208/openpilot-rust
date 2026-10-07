#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = ("openpilot-opencv-runtime", "openpilot-jpeg")


def artifacts(test_messages: str, example_messages: str) -> tuple[list[Path], dict[str, Path]]:
  rows = []
  for messages in [test_messages, example_messages]:
    batch = [json.loads(line) for line in messages.splitlines()]
    finished = [row for row in batch if row.get("reason") == "build-finished"]
    if len(finished) != 1 or finished[0].get("success") is not True:
      raise ValueError("expected one successful Cargo build in each receipt")
    rows.extend(batch)
  tests, examples = [], {}
  suites = set()
  for row in rows:
    if row.get("reason") != "compiler-artifact" or not row.get("executable"):
      continue
    package = next((name for name in PACKAGES if f"#{name}@" in row["package_id"]), None)
    if package is None:
      continue
    path = Path(row["executable"]).resolve(strict=True)
    if row["profile"]["test"]:
      suites.add((package, row["target"]["name"]))
      tests.append(path)
    elif "example" in row["target"]["kind"]:
      examples[row["target"]["name"]] = path
  required = {("openpilot-opencv-runtime", name) for name in ["native", "contracts"]}
  required.update(("openpilot-jpeg", name) for name in ["ownership", "options"])
  if not required <= suites or set(examples) != {"opencv_trace", "jpeg_options"}:
    raise ValueError("missing current OpenCV/JPEG test or example artifacts")
  return tests, examples


def main() -> None:
  parser = argparse.ArgumentParser(description=(
    "Instrument complete external OpenCV/JPEG and their native ownership boundaries; pure Rust is checked separately with Miri."))
  for name in ["opencv", "models", "target-dir", "output"]:
    parser.add_argument("--" + name, type=Path, required=True)
  args = parser.parse_args()
  for name in ["opencv", "models", "target_dir", "output"]:
    setattr(args, name, getattr(args, name).resolve())
  if shutil.disk_usage(args.output.parent).free < 25 * 1024**3 + 2 * 1024**3:
    raise OSError("memory validation requires 25 GiB plus 2 GiB; recover 35 GiB before resuming")
  args.output.mkdir()
  library = Path(subprocess.check_output(["clang-18", "--print-file-name=libclang_rt.asan-x86_64.so"], text=True).strip()).resolve(strict=True)
  flags = "-fsanitize=address,undefined -fno-omit-frame-pointer"
  environment = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir), CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="2",
    CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", CC="clang-18", CXX="clang++-18", CFLAGS=flags, CXXFLAGS=flags,
    RUSTFLAGS=f"-C link-arg={library}", OPENPILOT_OPENCV_ROOT=str(args.opencv))
  commands = []

  def run(name: str, command: list[str], env: dict[str, str]) -> str:
    result = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True, check=False)
    (args.output / (name + ".stdout")).write_text(result.stdout)
    (args.output / (name + ".stderr")).write_text(result.stderr)
    commands.append({"name": name, "argv": command, "returncode": result.returncode})
    (args.output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    result.check_returncode()
    return result.stdout

  common = ["--manifest-path", str(ROOT / "rust/Cargo.toml"), "-p", PACKAGES[0], "-p", PACKAGES[1],
    "--features", "native-skip-miri", "--locked", "--message-format=json"]
  messages = run("tests-build", ["cargo", "test", *common, "--lib", "--tests", "--no-run"], environment)
  example_messages = run("examples-build", ["cargo", "build", *common, "--examples"], environment)
  tests, examples = artifacts(messages, example_messages)
  runtime = {"LD_PRELOAD": str(library), "LD_LIBRARY_PATH": f"{args.opencv}/install/lib:{library.parent}",
    "ASAN_OPTIONS": "detect_leaks=1:halt_on_error=1", "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1",
    "OPENCV_TEST_LANE_MODEL": str(args.models / "lane.onnx")}
  for index, binary in enumerate(tests):
    run(f"native-test-{index}", [str(binary), "--nocapture", "--test-threads=1"], dict(environment, **runtime))
  cache = list(args.target_dir.glob("debug/build/openpilot-jpeg-*/out/jpeg/CMakeCache.txt"))
  if len(cache) != 1 or "CMAKE_C_FLAGS:STRING=" + flags not in cache[0].read_text():
    raise ValueError("complete JPEG codec sanitizer flags are missing or ambiguous")
  receipt = json.loads((args.opencv / "receipt.json").read_text())
  if receipt["status"] != "PASS" or receipt["sanitizers"] != "address,undefined":
    raise ValueError("OpenCV external build was not fully instrumented")
  run("source", [sys.executable, "-P", str(ROOT / "rust/tools/generate_xiaoge_opencv_reference.py"),
    "--models", str(args.models), "--output", str(args.output / "reference")], dict(os.environ))
  runner = ["--runner", "env", *[argument for key, value in runtime.items() for argument in ("--runner", f"{key}={value}")]]
  run("opencv-oracle", [sys.executable, "-P", str(ROOT / "rust/tools/check_xiaoge_opencv.py"),
    "--binary", str(examples["opencv_trace"]), "--fixtures", str(args.output / "reference/fixtures.json"),
    "--output", str(args.output / "opencv"), *runner], dict(os.environ))
  run("jpeg-oracle", [sys.executable, "-P", str(ROOT / "rust/tools/check_xiaoge_opencv_jpeg.py"),
    "--binary", str(examples["jpeg_options"]), "--output", str(args.output / "jpeg"), *runner], dict(os.environ))
  result = {"status": "PASS", "tests": len(tests), "full_codec_c_flags": flags,
    "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in [*tests, *examples.values()]},
    "scope": "ASan/UBSan/leak detection on complete native OpenCV/JPEG libraries and C/C++ boundaries; pure Rust Miri is separate"}
  (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
