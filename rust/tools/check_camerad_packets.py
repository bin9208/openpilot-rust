import argparse
import hashlib
import json
import random
import shlex
import subprocess
from pathlib import Path


def cases() -> list[dict]:
  rng = random.Random(187)
  result = [{"op": "layout"}, {"op": "csiphy"}]
  result += [{"op": "probe", "sensor": sensor, "port": port} for sensor in [1, 2, 3] for port in range(3)]
  result += [{"op": "poke", "request": request} for request in [-(2**31), -1, 0, 1, 2**31 - 1]]
  result += [{"op": "poke", "request": rng.randrange(-(2**31), 2**31)} for _ in range(300)]
  for words in [False, True]:
    for count in [0, 1, 3, 9, 31, 64, 221, 512, 65535, 65536, 65537]:
      result.append({"op": "i2c", "words": words, "opcode": 4, "registers": [[rng.getrandbits(32), rng.getrandbits(32)] for _ in range(count)]})
    for _ in range(300):
      result.append(
        {
          "op": "i2c",
          "words": words,
          "opcode": rng.getrandbits(32),
          "registers": [[rng.getrandbits(32), rng.getrandbits(32)] for _ in range(rng.randrange(150))],
        }
      )
  for sensor in [1, 2, 3]:
    for raw in [False, True]:
      for phy in [0, 0x4001, 0x4002, 0x4003, 0xFFFFFFFF]:
        for width, height in [(1928, 1208), (1344, 760)]:
          for handle in [-1, 0, 2**31 - 1]:
            result.append({"op": "acquire", "sensor": sensor, "raw": raw, "phy": phy, "width": width, "height": height, "handle": handle, "size": 768})
    result.append({"op": "bps_tables", "sensor": sensor})
    for slot in [0, 1, 17, 19]:
      for request in [-(2**31), -1, 0, 1, 2**31 - 1]:
        for width, height in [(1928, 1208), (1344, 760), (2, 2), (1929, 1209)]:
          result.append({"op": "bps", "sensor": sensor, "slot": slot, "request": request, "width": width, "height": height})
    for raw in [False, True]:
      for vignetting in [False, True]:
        for initial in [False, True]:
          for slot in [0, 1, 17, 19]:
            for request in [-(2**31), -1, 0, 1, 2**31 - 1]:
              for width, height in [(1928, 1208), (1344, 760), (2, 2), (1929, 1209)]:
                result.append(
                  {
                    "op": "ife",
                    "sensor": sensor,
                    "raw": raw,
                    "vignetting": vignetting,
                    "slot": slot,
                    "request": request,
                    "initial": initial,
                    "width": width,
                    "height": height,
                  }
                )
  return result


def signed(value: int) -> int:
  return value if value < 2**31 else value - 2**32


def source_line(case: dict) -> str:
  match case["op"]:
    case "layout" | "csiphy":
      return case["op"]
    case "probe":
      return f"probe {case['sensor']} {case['port']}"
    case "poke":
      return f"poke {case['request']}"
    case "i2c":
      prefix = f"i2c {int(case['words'])} {signed(case['opcode'])} {len(case['registers'])}"
      return prefix + " " + " ".join(str(value) for pair in case["registers"] for value in pair)
    case "ife":
      return "ife " + " ".join(str(int(case[key])) for key in ["sensor", "raw", "vignetting", "slot", "request", "initial", "width", "height"])
    case "bps":
      return f"bps {case['sensor']} 0 0 {case['slot']} {case['request']} 0 {case['width']} {case['height']}"
    case "bps_tables":
      return f"bps_tables {case['sensor']}"
    case "acquire":
      return "acquire " + " ".join(str(int(case[key])) for key in ["sensor", "raw", "phy", "width", "height", "handle", "size"])
    case _:
      raise ValueError(case)


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare complete sensor probe, I2C, NOP and CSI PHY packet bytes with unchanged source methods")
  parser.add_argument("--reference", type=Path, required=True)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--reference-runner", default="")
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  inputs = cases()
  (args.output / "inputs.jsonl").write_text("".join(json.dumps(case) + "\n" for case in inputs))
  source_cmd = shlex.split(args.reference_runner) + [str(args.reference)]
  native_cmd = shlex.split(args.runner) + [str(args.binary)]
  byte_count = 0
  for start in range(0, len(inputs), 50):
    batch = inputs[start : start + 50]
    original = subprocess.run(source_cmd, input="\n".join(map(source_line, batch)) + "\n", text=True, capture_output=True, timeout=120)
    native = subprocess.run(native_cmd, input="".join(json.dumps(case) + "\n" for case in batch), text=True, capture_output=True, timeout=120)
    for label, process in [("source", original), ("native", native)]:
      (args.output / f"batch-{start:03d}.{label}.jsonl").write_text(process.stdout)
      (args.output / f"batch-{start:03d}.{label}.stderr").write_text(process.stderr)
      process.check_returncode()
    left, right = [[json.loads(line) for line in process.stdout.splitlines()] for process in [original, native]]
    assert len(left) == len(right) == len(batch)
    for index, (source, observed) in enumerate(zip(left, right, strict=True)):
      if source != observed:
        (args.output / "failure.json").write_text(json.dumps({"index": start + index, "case": batch[index], "source": source, "native": observed}, indent=2))
        raise AssertionError(f"packet mismatch at {start + index}, {batch[index]['op']}")
      if batch[index]["op"] != "layout":
        byte_count += sum(map(len, source))
  report = {
    "status": "PASS",
    "cases": len(inputs),
    "compared_bytes": byte_count,
    "source_command": source_cmd,
    "native_command": native_cmd,
    "source_sha256": hashlib.sha256(args.reference.read_bytes()).hexdigest(),
    "native_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "scope": "Complete zero-initialized packet/payload bytes from original methods; controlled memory handles; no live kernel/device claim",
  }
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
