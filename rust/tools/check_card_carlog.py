from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import math
import os
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import uuid


def cases() -> list[dict[str, str | float]]:
  points: set[int] = {0, 32, 127, 0x10ffff}
  previous = True
  for point in range(0x110000):
    printable = chr(point).isprintable()
    if printable != previous:
      points.update([point - 1, point])
    previous = printable
  points.update(random.Random(177).sample(range(0x110000), 400))
  result: list[dict[str, str | float]] = [{"op": "malformed_vin", "vin": chr(point)} for point in sorted(points)
                                       if 0 <= point <= 0x10ffff and not 0xd800 <= point <= 0xdfff]
  result.extend({"op": "malformed_vin", "vin": value} for value in ["plain", "can't", 'a"b', "a'\"b", "\t\n\r\\", "한글"])
  result.extend([{"op": "text", "level": "warning", "message": "CANParser: counter invalid"},
                 {"op": "text", "level": "error", "message": "FSD 14 detected, but FW not in FSD_14_FW set"},
                 {"op": "unmatched", "fingerprints": "{0: {123: 8}, 2: {}}"}])
  numbers = [0., -0., 1., 1e-5, 1e-4, 1e15, 1e16, 5e-324]
  generator = random.Random(177)
  for _ in range(1000):
    number = struct.unpack("d", generator.getrandbits(64).to_bytes(8, sys.byteorder))[0]
    if math.isfinite(number):
      numbers.append(number)
  result.extend({"op": "fingerprinted", "time": number} for number in numbers)
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  arguments = parser.parse_args()
  output = arguments.output.resolve()
  free = shutil.disk_usage(output.parent).free
  assert free >= 25 * 1024**3 + 128 * 1024**2, free
  output.mkdir(parents=True, exist_ok=True)
  inputs = cases()
  (output / "inputs.json").write_text(json.dumps(inputs) + "\n")
  levels: list[str | bytes | None] = [None, "info", "warning", "error", "critical", "debug", "INFO", "WARN", "FATAL", "NOTSET",
                                    "", "10", "unknown", b"\xff"]
  rows = []
  with tempfile.TemporaryDirectory(prefix="card-carlog-") as temporary:
    for index, level in enumerate(levels):
      directory = output / str(index)
      directory.mkdir()
      environment: dict[str, str | bytes] = dict(os.environ)
      prefix = f"carlog-{uuid.uuid4()}"
      environment.update(PARAMS_ROOT=temporary, OPENPILOT_PREFIX=prefix)
      if level is None:
        environment.pop("LOGPRINT", None)
      else:
        environment["LOGPRINT"] = level
      with ExitStack() as stack:
        queue_root = Path("/dev/shm") / ("msgq_" + prefix)
        queue_root.mkdir()
        stack.callback(shutil.rmtree, queue_root)
        source = subprocess.run([sys.executable, "rust/tools/card_carlog_source.py"], input=json.dumps(inputs).encode(),
                                capture_output=True, env=environment, check=False)
        native = subprocess.run([str(arguments.binary.resolve())], input=json.dumps(inputs).encode(),
                                capture_output=True, env=environment, check=False)
      for name, process in [("source", source), ("native", native)]:
        (directory / f"{name}.stdout").write_bytes(process.stdout)
        (directory / f"{name}.stderr").write_bytes(process.stderr)
        (directory / f"{name}.exit").write_text(str(process.returncode) + "\n")
      valid_level = index < 10
      row = {"level": level.hex() if isinstance(level, bytes) else level, "source_exit": source.returncode, "native_exit": native.returncode,
             "stderr_equal": source.stderr == native.stderr, "stdout_equal": source.stdout == native.stdout,
             "valid_level": valid_level}
      rows.append(row)
      (output / "result.json").write_text(json.dumps({"cases_per_level": len(inputs), "levels": rows,
            "binary_sha256": hashlib.sha256(arguments.binary.read_bytes()).hexdigest()}, indent=2) + "\n")
      if valid_level:
        assert source.returncode == native.returncode == 0, row
        assert row["stderr_equal"] and row["stdout_equal"], row
      else:
        assert source.returncode != 0 and native.returncode != 0, row
  print(f"PASS {len(inputs)} exact console cases x10 valid levels +4 explicit invalid-level failures")


if __name__ == "__main__":
  main()
