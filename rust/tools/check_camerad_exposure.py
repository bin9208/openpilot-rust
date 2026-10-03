import argparse
import hashlib
import json
from pathlib import Path
import random
import shlex
import subprocess
from typing import cast

from check_camerad_sensors import equal


def quoted(value: str) -> str:
  return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare continuous exposure state and writes with original C++ methods")
  parser.add_argument("--reference", type=Path, required=True)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--reference-runner", default="")
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--frames", type=int, default=1200)
  args = parser.parse_args()
  if not 1 <= args.frames <= 1200:
    parser.error("--frames must be between 1 and 1200")
  reference, binary, output = cast(Path, args.reference), cast(Path, args.binary), cast(Path, args.output)
  rng = random.Random(1871)
  source_lines: list[str] = []
  native_lines: list[str] = []
  steps = 0
  scenarios = 0
  for sensor, gain_max, max_time in [(1, 13, 2133), (2, 54, 2016), (3, 40, 2352)]:
    for camera, focal in [(0, 1.71), (1, 8.0), (2, 1.71)]:
      width, height = (1344, 760) if sensor == 3 else (1928, 1208)
      for mode in range(5):
        scenarios += 1
        reset = {"op": "reset", "sensor": sensor, "camera": camera, "width": width, "height": height, "focal": focal}
        source_lines.append(f"reset {sensor} {camera} {width} {height} {focal}")
        native_lines.append(json.dumps(reset))
        for frame in range(args.frames):
          match mode:
            case 0:
              grey = [0.0, 0.99609375, 0.125, 0.5][(frame // 300) % 4]
            case 1:
              grey = rng.randrange(256) / 256
            case 2:
              grey = 0.01 if (frame // 100) % 2 else 0.95
            case 3:
              grey = 0.1 + 0.05 * (frame % 19) / 19
            case 4:
              grey = 0.125
            case _:
              raise AssertionError(mode)
          enabled = frame % 7 != 0
          gain, time = "", ""
          if mode == 4:
            gain, time = str(rng.randrange(gain_max + 1)), str(rng.randrange(2, max_time + 1))
            if frame % 5 == 0:
              gain, time = "\t +" + gain + "suffix", " " + time + " trailing"
            if frame % 13 == 0:
              gain = ""
            if frame % 17 == 0:
              time = ""
          frame_id = (frame + (4294967100 if mode == 3 else 1)) % 2**32
          step = {"op": "step", "frame_id": frame_id, "grey": grey, "enabled": enabled, "gain": gain, "time": time}
          source_lines.append(f"step {frame_id} {grey:.17g} {int(enabled)} {quoted(gain)} {quoted(time)}")
          native_lines.append(json.dumps(step))
          steps += 1
      for gain, time in [("x", "1"), ("1", "x"), ("99999999999999", "1"), ("1", "999999999999"), ("1", "+"), ("  ", "5")]:
        scenarios += 1
        source_lines.extend([f"reset {sensor} {camera} {width} {height} {focal}", f"step 1 0.125 1 {quoted(gain)} {quoted(time)}", 'step 2 0.125 1 "" ""'])
        native_lines.extend(
          [
            json.dumps(reset),
            json.dumps({"op": "step", "frame_id": 1, "grey": 0.125, "enabled": True, "gain": gain, "time": time}),
            json.dumps({"op": "step", "frame_id": 2, "grey": 0.125, "enabled": True, "gain": "", "time": ""}),
          ]
        )
        steps += 2
  source_input, native_input = "\n".join(source_lines) + "\n", "\n".join(native_lines) + "\n"
  source_command = shlex.split(args.reference_runner) + [str(reference.resolve())]
  source_result = subprocess.run(source_command, input=source_input, text=True, capture_output=True, check=True)
  command = shlex.split(args.runner) + [str(binary.resolve())]
  native_result = subprocess.run(command, input=native_input, text=True, capture_output=True, check=True)
  source_output, native_output = source_result.stdout.splitlines(), native_result.stdout.splitlines()
  assert len(source_output) == len(native_output) == len(source_lines)
  output.mkdir(parents=True, exist_ok=True)
  for index, (source, native) in enumerate(zip(source_output, native_output, strict=True)):
    try:
      equal(json.loads(source), json.loads(native), f"step[{index}]")
    except AssertionError as error:
      (output / "failure.json").write_text(
        json.dumps({"index": index, "request": native_lines[index], "source": json.loads(source), "native": json.loads(native), "error": str(error)}, indent=2)
        + "\n"
      )
      raise
  report = {
    "status": "PASS",
    "scenarios": scenarios,
    "steps": steps,
    "observations": len(source_lines),
    "source_command": source_command,
    "native_command": command,
    "source_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
    "native_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_input_sha256": hashlib.sha256(source_input.encode()).hexdigest(),
    "native_input_sha256": hashlib.sha256(native_input.encode()).hexdigest(),
    "comparison": "full exposure state, rectangle and register writes; float values compared as binary32",
    "limits": "original methods with controlled sensor/Params/write boundary; no camera hardware",
  }
  (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
