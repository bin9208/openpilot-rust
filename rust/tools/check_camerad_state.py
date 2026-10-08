import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess

from check_camerad_exposure import quoted
from check_camerad_sensors import equal


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original camera state wire payload and pre-publication exposure writes")
  for name in ["source", "native", "output", "schema"]:
    parser.add_argument(f"--{name}", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--source-runner", default="")
  parser.add_argument("--frames", type=int, default=120)
  args = parser.parse_args()
  assert 1 <= args.frames <= 400
  import capnp

  schema = capnp.load(str(args.schema.resolve()), imports=[str(args.schema.parent.resolve())])
  source_lines, native_lines = [], []
  for sensor in range(1, 4):
    width, height = (1344, 760) if sensor == 3 else (1928, 1208)
    for camera, focal in [(0, 1.71), (1, 8.0), (2, 1.71)]:
      reset = {"op": "reset", "sensor": sensor, "camera": camera, "width": width, "height": height, "focal": focal}
      source_lines.append(f"reset {sensor} {camera} {width} {height} {focal}")
      native_lines.append(json.dumps(reset))
      for step in range(args.frames):
        frame_id = step if step < 100 else (2**32 - 9 + step - 100) % 2**32
        gain, time = ("0", "2309") if step % 19 == 5 else ("", "")
        if step % 19 == 8:
          gain, time = "  +0tail", "\t 37suffix"
        if step % 31 == 7:
          gain, time = "x", "1"
        if step % 31 == 9:
          gain, time = "1", "99999999999999999999"
        row = {
          "op": "step",
          "frame_id": frame_id,
          "request_id": (step * 27 + 2**32 - 7) % 2**32,
          "sof": 1_000_000_000 + step * 50_000_000,
          "eof": 1_011_000_000 + step * 50_000_000,
          "processing": (step % 17) / 1024,
          "log_time": 1_025_000_000 + step * 50_000_000,
          "seed": [0, 255, 32, 100][step // 20 % 4],
          "pattern": step % 13 == 6,
          "log_raw": step % 23 != 5,
          "enabled": step % 11 != 4,
          "gain": gain,
          "time": time,
        }
        values = [row[key] for key in ["frame_id", "request_id", "sof", "eof", "processing", "log_time", "seed"]]
        values += [int(row[key]) for key in ["pattern", "log_raw", "enabled"]]
        source_lines.append("step " + " ".join(map(str, values)) + " " + quoted(gain) + " " + quoted(time))
        native_lines.append(json.dumps(row))
  args.output.mkdir(parents=True, exist_ok=True)
  outputs = []
  runs = []
  for lane, binary, runner, lines in [("source", args.source, args.source_runner, source_lines), ("native", args.native, args.runner, native_lines)]:
    text = "\n".join(lines) + "\n"
    (args.output / f"{lane}.input").write_text(text)
    command = shlex.split(runner) + [str(binary)]
    process = subprocess.run(command, input=text, capture_output=True, text=True, timeout=180)
    (args.output / f"{lane}.stdout").write_text(process.stdout)
    (args.output / f"{lane}.stderr").write_text(process.stderr)
    runs.append({"command": command, "exit": process.returncode, "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()})
    (args.output / "runs.json").write_text(json.dumps(runs, indent=2) + "\n")
    process.check_returncode()
    assert not any(value in process.stderr for value in ["ERROR: AddressSanitizer", "runtime error:"])
    rows = [json.loads(line) for line in process.stdout.splitlines()]
    for row in rows:
      if "wire" in row:
        with schema.Event.from_bytes(bytes(row["wire"])) as event:
          row["wire"] = event.to_dict()
          for service in ["roadCameraState", "wideRoadCameraState", "driverCameraState"]:
            if service in row["wire"] and "image" in row["wire"][service]:
              row["wire"][service]["image"] = list(row["wire"][service]["image"])
    (args.output / f"{lane}.fields.json").write_text(json.dumps(rows) + "\n")
    outputs.append(rows)
  assert len(outputs[0]) == len(outputs[1]) == 9 * args.frames
  try:
    equal(outputs[0], outputs[1], "camera_state")
  except AssertionError as error:
    (args.output / "failure.txt").write_text(str(error) + "\n")
    raise
  result = {
    "status": "pass",
    "frames": len(outputs[0]),
    "sensors": 3,
    "cameras": 3,
    "runs": runs,
    "scope": "original sendState wire fields, image decimation, pre-AE snapshot, post-AE state and writes, transport order, manual exposure failures",
  }
  (args.output / "report.json").write_text(json.dumps(result, indent=2) + "\n")
  print(json.dumps(result))


if __name__ == "__main__":
  main()
