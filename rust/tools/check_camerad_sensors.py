"""Compare sensor metadata, register writes and score float bits with original C++."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import random
import shlex
import struct
import subprocess
from typing import cast


def f32(value: float) -> bytes:
  return struct.pack("<f", value)


def run(command: list[str], requests: str) -> list[object]:
  result = subprocess.run(command, input=requests, text=True, capture_output=True, check=True)
  return [json.loads(line) for line in result.stdout.splitlines()]


def equal(source: object, native: object, path: str) -> None:
  if isinstance(source, dict):
    assert isinstance(native, dict), (path, source, native)
    assert source.keys() == native.keys(), (path, source.keys(), native.keys())
    for key, value in source.items():
      equal(value, native[key], f"{path}.{key}")
  elif isinstance(source, list):
    assert isinstance(native, list) and len(source) == len(native), (path, source, native)
    for index, (left, right) in enumerate(zip(source, native, strict=True)):
      equal(left, right, f"{path}[{index}]")
  elif isinstance(source, float):
    assert isinstance(native, (float, int)), (path, source, native)
    assert f32(source) == f32(native), (path, source, native, f32(source).hex(), f32(native).hex())
  else:
    assert source == native, (path, source, native)


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--reference", type=Path, required=True)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--reference-runner", default="")
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  reference, binary, output = cast(Path, args.reference), cast(Path, args.binary), cast(Path, args.output)
  source_command = shlex.split(args.reference_runner) + [str(reference.resolve())]
  native_command = shlex.split(args.runner) + [str(binary.resolve())]
  configs = cast(list[dict[str, object]], run(source_command, "config 1\nconfig 2\nconfig 3\n"))
  requests = [f"config {kind}" for kind in range(1, 4)]
  rng = random.Random(187)
  counts: Counter[str] = Counter()
  for kind, config in enumerate(configs, 1):
    requests.extend(f"address {kind} {port}" for port in range(3))
    gains = cast(list[float], config["sensor_analog_gains"])
    maximum = cast(int, config["exposure_time_max"])
    for time in range(2, maximum + 1):
      for gain in range(len(gains)):
        for dc_gain in range(2):
          requests.append(f"exposure {kind} {time} {gain} {dc_gain}")
    for time in [-2147483648, -65537, -1, 0, 1, 65535, 65536, 2147483647]:
      for gain in range(len(gains)):
        requests.append(f"exposure {kind} {time} {gain} 0")
    for _ in range(20000):
      gain_index = rng.randrange(len(gains))
      gain = gains[gain_index] * rng.choice([1.0, cast(float, config["dc_gain_factor"])])
      desired = rng.uniform(0, cast(float, config["max_ev"]) * 1.2)
      time, previous = rng.randrange(2, maximum + 1), rng.randrange(len(gains))
      requests.append(f"score {kind} {desired:.17g} {time} {gain_index} {gain:.17g} {previous}")
  output.mkdir(parents=True, exist_ok=True)
  digest = hashlib.sha256()
  for start in range(0, len(requests), 2000):
    batch = requests[start : start + 2000]
    inputs = "\n".join(batch) + "\n"
    source, native = run(source_command, inputs), run(native_command, inputs)
    assert len(source) == len(native) == len(batch)
    for request, left, right in zip(batch, source, native, strict=True):
      try:
        equal(left, right, request)
      except AssertionError as error:
        (output / "failure.json").write_text(json.dumps({"request": request, "source": left, "native": right, "error": str(error)}, indent=2) + "\n")
        raise
      counts[request.split()[0]] += 1
    digest.update(inputs.encode())
  report = {
    "status": "PASS",
    "cases": len(requests),
    "counts": dict(counts),
    "comparison": "exact registers/integer data and bit-identical binary32 sensor/score values",
    "input_sha256": digest.hexdigest(),
    "seed": 187,
    "source_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
    "native_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_command": source_command,
    "native_command": native_command,
    "limits": "host or explicit emulator only; no camera hardware or complete runtime startup",
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
