from __future__ import annotations

import argparse
import bz2
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import zstandard

from cluster_route_frames_compare import native_frame
from cluster_simulator_compare import compare


def run_native(binary, mode, paths, output):
  request = {"mode": mode, "paths": list(map(str, paths))}
  result = subprocess.run([str(binary)], input=json.dumps(request), capture_output=True, text=True)
  (output / f"{mode}-request.json").write_text(json.dumps(request, indent=2))
  (output / f"{mode}-native.json").write_text(result.stdout)
  (output / f"{mode}-native.stderr").write_text(result.stderr)
  result.check_returncode()
  return json.loads(result.stdout)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--root", type=Path, required=True)
  parser.add_argument("--native", type=Path, required=True)
  parser.add_argument("--steps", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.root, args.output, args.native = args.root.resolve(), args.output.resolve(), args.native.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(args.root))
  sys.path.insert(0, str(args.root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_route_replay as source

  payload = bytes(range(256)) * 4
  bz = bz2.compress(payload)
  zstd = zstandard.ZstdCompressor().compress(payload)
  large = bytes((index * 251 + index // 17) % 256 for index in range(300_000))
  large_zstd = zstandard.ZstdCompressor(write_checksum=True).compress(large)
  cases = [
    ("raw", payload),
    ("empty.raw", b""),
    ("normal.bz2", bz),
    ("bzip.magic", bz),
    ("normal.zst", zstd),
    ("zstd.magic", zstd),
    ("multi.bz2", bz + bz2.compress(b"tail")),
    ("multi.zst", zstd + zstandard.ZstdCompressor().compress(b"tail")),
    ("bzip.zst", bz),
    ("zstd.bz2", zstd),
    ("empty.bz2", b""),
    ("empty.zst", b""),
    ("junk.bz2", b"junk"),
    ("junk.zst", b"junk"),
    ("bzip-header", b"BZh"),
    ("zstd-header", zstd[:4]),
    ("bzip-truncated", bz[:-1]),
    ("zstd-truncated", zstd[:-1]),
    ("bzip-trailing", bz + b"trailing"),
    ("zstd-trailing", zstd + b"trailing"),
    ("large.zst", large_zstd),
    ("large-truncated.zst", large_zstd[:-1]),
    ("large-half.zst", large_zstd[: len(large_zstd) // 2]),
    ("bzip-empty-first.bz2", bz2.compress(b"") + b"trailing"),
    ("bzip-second-truncated.bz2", bz + bz[:-1]),
    ("bzip-second-invalid.bz2", bz + bz[:20] + b"corrupt-data"),
  ]
  paths, expected_bytes = [], []
  for name, data in cases:
    path = args.output / name
    path.write_bytes(data)
    paths.append(path)
    try:
      expected_bytes.append({"Ok": list(source.read_log_bytes(path))})
    except Exception as error:
      expected_bytes.append({"Err": f"{type(error).__name__}: {error}"})
  (args.output / "bytes-source.json").write_text(json.dumps(expected_bytes))
  actual_bytes = run_native(args.native, "bytes", paths, args.output)
  byte_failures = []
  for index, (expected, actual) in enumerate(zip(expected_bytes, actual_bytes, strict=True)):
    if ("Ok" in expected) != ("Ok" in actual) or ("Ok" in expected and expected["Ok"] != actual["Ok"]):
      byte_failures.append({"case": cases[index][0], "source": expected, "native": actual})

  steps = json.loads(args.steps.read_text())
  log_paths, expected_frames = [], []
  original = source.RouteLogParser(recompute_cutins=False)
  schema = source.load_openpilot_log_schema()
  for index, subset in enumerate((steps[:20], steps[20:45], steps[45:])):
    data = b"".join(Path(path).read_bytes() for step in subset for path in step["messages"])
    filename = ("segment0.zst", "segment1.bz2", "segment2.raw")[index]
    path = args.output / filename
    path.write_bytes((zstandard.ZstdCompressor().compress(data) if index == 0 else bz2.compress(data)) if index < 2 else data)
    frames = original.parse_file(path, schema)
    expected_frames.append([native_frame(frame, source.frame_to_state) for frame in frames])
    log_paths.append(path)
  (args.output / "parse-source.json").write_text(json.dumps(expected_frames))
  actual_frames = run_native(args.native, "parse", log_paths, args.output)
  frame_failures = []
  for index, (expected_file, actual_file) in enumerate(zip(expected_frames, actual_frames, strict=True)):
    for frame, (expected, actual) in enumerate(zip(expected_file, actual_file, strict=True)):
      try:
        compare(json.loads(json.dumps(expected)), actual)
      except (AssertionError, TypeError) as error:
        frame_failures.append({"file": index, "frame": frame, "detail": str(error)})
  report = {
    "byte_cases": len(cases),
    "files": len(log_paths),
    "frames": sum(map(len, expected_frames)),
    "byte_failures": byte_failures,
    "frame_failures": frame_failures,
    "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256(Path(source.__file__).read_bytes()).hexdigest(),
  }
  (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
  summary = {
    **report,
    "byte_failures": [
      {
        "case": failure["case"],
        "source": len(failure["source"]["Ok"]) if "Ok" in failure["source"] else "error",
        "native": len(failure["native"]["Ok"]) if "Ok" in failure["native"] else "error",
      }
      for failure in byte_failures
    ],
  }
  print(json.dumps(summary))
  if byte_failures or frame_failures:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
