import argparse
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path
import random
import shlex
import subprocess
from typing import cast

from check_camerad_sensors import equal


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare camera packing, allocation, histogram and passive timing with C++")
  parser.add_argument("--reference", type=Path, required=True)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--reference-runner", default="")
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  reference, binary, output = cast(Path, args.reference), cast(Path, args.binary), cast(Path, args.output)
  output.mkdir(parents=True, exist_ok=True)
  fixtures = output / "fixtures"
  fixtures.mkdir(exist_ok=True)
  rng = random.Random(1872)
  pairs: list[tuple[str, dict[str, object]]] = []
  for index in range(5000):
    sof = 1_000_000_000 + index * 50_000_000
    if index % 29 == 0:
      sof -= 200_000_000
    received = sof + rng.choice([0, 1, 30_000_000, 75_000_000, 75_000_001, 200_000_000])
    pairs.append((f"timing {sof} {received}", {"op": "timing", "sof": sof, "received": received}))
  for sof, received in [(0, 0), (0, 200_000_000), (2**64 - 1, 0), (0, 2**64 - 1), (1, 2), (10**12, 10**12), (10**12, 10**12 + 100_000_000)]:
    pairs.append((f"timing {sof} {received}", {"op": "timing", "sof": sof, "received": received}))
  widths = [0, 1, 127, 128, 129, 511, 512, 513, 1344, 1928, 2688, 4096]
  heights = [0, 1, 31, 32, 33, 63, 64, 65, 760, 1208, 1520, 3072]
  for width, height in itertools.product(widths, heights):
    pairs.append((f"nv12 {width} {height}", {"op": "nv12", "width": width, "height": height}))
  for _ in range(1500):
    width, height = rng.randrange(1, 4097), rng.randrange(1, 3073)
    pairs.append((f"nv12 {width} {height}", {"op": "nv12", "width": width, "height": height}))
  for length in [0, 1, 2, 255, 256, 65535, 65536, 65537, 2**32 - 1]:
    for address in [0, 1, 0xFFFFFF, 0x1000000, 2**32 - 1]:
      for selector, opcode in [(0, 10), (1, 11), (255, 255), (12, 1)]:
        pairs.append(
          (f"dmi {length} {address} {selector} {opcode}", {"op": "dmi", "length": length, "address": address, "selector": selector, "opcode": opcode})
        )
  for count in list(range(130)) + [255, 256, 257, 1023, 1024, 1025, 65535, 65536, 65537]:
    values = [rng.randrange(2**32) for _ in range(count)]
    address = rng.randrange(2**32)
    for op in ("cont", "random"):
      fields: dict[str, object] = {"op": op, "values": values}
      if op == "cont":
        fields["address"] = address
      pairs.append((f"{op} {address} {count} " + " ".join(map(str, values)), fields))
  images = []
  for splits in itertools.product(range(3), repeat=4):
    remaining = 160
    rows = []
    for split in splits:
      count = split * remaining // 3
      rows.append(count)
      remaining -= count
    rows.append(remaining)
    pixels = b"".join(bytes([value]) * (row * 240) for value, row in zip([0, 24, 48, 96, 235], rows, strict=True))
    images.append((240, pixels, [0, 0, 239, 159], 1, 1))
  for _ in range(100):
    width, height = rng.randrange(1, 128), rng.randrange(1, 96)
    pixels = rng.randbytes(width * height)
    x, y = rng.randrange(width), rng.randrange(height)
    region = [x, y, rng.randrange(width - x + 1), rng.randrange(height - y + 1)]
    images.append((width, pixels, region, rng.randrange(1, 5), rng.randrange(1, 5)))
  images.extend([(1, b"\x11", [0, 0, 1, 1], 1, 1), (4, b"\x00\x01\xc8\xff", [0, 0, 4, 1], 1, 1)])
  for index, (width, pixels, region, xskip, yskip) in enumerate(images):
    path = fixtures / f"image-{index}.bin"
    path.write_bytes(pixels)
    pairs.append(
      (
        f"luminance {width} " + " ".join(map(str, region)) + f' {xskip} {yskip} "{path.resolve()}"',
        {"op": "luminance", "width": width, "region": region, "x_skip": xskip, "y_skip": yskip, "file": str(path.resolve())},
      )
    )
  source_input = "\n".join(source for source, _ in pairs) + "\n"
  native_input = "\n".join(json.dumps(native) for _, native in pairs) + "\n"
  source_command = shlex.split(args.reference_runner) + [str(reference.resolve())]
  source_result = subprocess.run(source_command, input=source_input, text=True, capture_output=True, check=True)
  command = shlex.split(args.runner) + [str(binary.resolve())]
  native_result = subprocess.run(command, input=native_input, text=True, capture_output=True, check=True)
  source_lines, native_lines = source_result.stdout.splitlines(), native_result.stdout.splitlines()
  assert len(source_lines) == len(native_lines) == len(pairs)
  for index, (source, native) in enumerate(zip(source_lines, native_lines, strict=True)):
    try:
      equal(json.loads(source), json.loads(native), f"case[{index}]")
    except AssertionError as error:
      (output / "failure.json").write_text(
        json.dumps({"input": pairs[index], "source": json.loads(source), "native": json.loads(native), "error": str(error)}, indent=2) + "\n"
      )
      raise
  report = {
    "status": "PASS",
    "counts": dict(Counter(cast(str, native["op"]) for _, native in pairs)),
    "source_command": source_command,
    "native_command": command,
    "source_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
    "native_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_input_sha256": hashlib.sha256(source_input.encode()).hexdigest(),
    "native_input_sha256": hashlib.sha256(native_input.encode()).hexdigest(),
    "limits": "synthetic host/explicit emulator only; no hardware, source C++ histogram and packing unmodified",
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
