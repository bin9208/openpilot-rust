import argparse
import hashlib
import json
import random
import shlex
import subprocess
from pathlib import Path


def reset(depth: int = 18, enabled: int = 7, bps: int = 7, ar: int = 0, staggered: int = 0, readout: int = 22_500_000) -> dict:
  return {"op": "reset", "depth": depth, "enabled": enabled, "bps": bps, "ar": ar, "staggered": staggered, "readout": readout}


def step(camera: int, request: int, frame: int, timestamp: int, failures: int = 0, stress: int = 0, **extra: int) -> dict:
  return {
    "op": "step",
    "camera": camera,
    "request": request,
    "frame": frame,
    "timestamp": timestamp,
    "status": extra.get("status", 0),
    "first_now": extra.get("first_now", timestamp + 25_000_000),
    "later_now": extra.get("later_now", timestamp + 30_000_000),
    "failures": failures,
    "stress": stress,
  }


def scenarios() -> list[dict]:
  cases = []
  for depth in [1, 3, 18]:
    for enabled in [1, 3, 7]:
      for bps in [0, 5, 7]:
        for ar in [0, 4]:
          for staggered in [0, 4]:
            for skew in [0, 200_000, 200_001]:
              commands = [reset(depth, enabled, bps, ar, staggered)]
              for frame in range(1, 46):
                for camera in range(3):
                  if not enabled & (1 << camera):
                    continue
                  offset = 25_000_000 if staggered & (1 << camera) and not ar & (1 << camera) else 0
                  timestamp = 1_000_000_000 + frame * 50_000_000 + offset + (skew if camera == 2 else 0)
                  commands.append(step(camera, frame, frame, timestamp))
              cases.append({"name": f"alignment-{depth}-{enabled}-{bps}-{ar}-{staggered}-{skew}", "commands": commands})
  rng = random.Random(187)
  for depth in [1, 3, 18]:
    for bps in [0, 7]:
      for trial in range(30):
        commands = [reset(depth, bps=bps)]
        requests = [1, 1, 1]
        frames = [1, 1, 1]
        for index in range(240):
          camera = index % 3
          timestamp = 1_000_000_000 + (index // 3) * 50_000_000
          draw = rng.randrange(20)
          request = requests[camera]
          frame = frames[camera]
          failures = stress = 0
          if draw == 0:
            request = 0
          elif draw == 1:
            timestamp -= 100_000_000
          elif draw == 2:
            frame += 2
          elif draw == 3:
            request += 2
          elif draw in [4, 5]:
            failures = draw - 3
          elif draw in [6, 7, 8, 9]:
            stress = 1 << (draw - 6)
          commands.append(step(camera, request, frame, timestamp, failures, stress, status=rng.randrange(4)))
          requests[camera] = max(requests[camera], request) + 1
          frames[camera] = frame + 1
        cases.append({"name": f"faults-{depth}-{bps}-{trial}", "commands": commands})
    commands = [reset(depth, enabled=1)]
    commands += [step(0, 0, index, 1_000_000_000 + index * 50_000_000) for index in range(depth * 3 + 12)]
    commands += [step(0, index, 100 + index, 10_000_000_000 + index * 50_000_000) for index in range(1, 8)]
    cases.append({"name": f"invalid-counter-{depth}", "commands": commands})
  maximum = 2**64 - 1
  commands = [reset(enabled=1, bps=0, readout=25_000_000)]
  for index, (request, frame) in enumerate([(maximum - 1, maximum - 2), (maximum, maximum - 1), (0, maximum), (1, 0), (2, 1)]):
    timestamp = 1_000_000_000 + index * 50_000_000
    commands.append(step(0, request, frame, timestamp, later_now=timestamp - 1))
  cases.append({"name": "u64-wrap-and-negative-processing-delay", "commands": commands})
  for request in [maximum - 18, maximum - 2, maximum]:
    cases.append({"name": f"requeue-overflow-{request}", "commands": [reset(), step(0, request, 42, 1_000_000_000, failures=1)]})
  cases.append({"name": "late-camera-after-sync-timeout", "commands": [reset(), step(0, 1, 41, 1_000_000_000), step(2, 1, 99, 1_100_000_000)]})
  return cases


def source_line(command: dict) -> str:
  keys = (
    ["depth", "enabled", "bps", "ar", "staggered", "readout"]
    if command["op"] == "reset"
    else [
      "camera",
      "request",
      "frame",
      "timestamp",
      "status",
      "first_now",
      "later_now",
      "failures",
      "stress",
    ]
  )
  return " ".join([command["op"], *(str(command[key]) for key in keys)])


def sha(path: Path) -> str:
  return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
  parser = argparse.ArgumentParser(description="Compare original Spectra frame decisions, state and ordered I/O boundary calls")
  parser.add_argument("--reference", type=Path, required=True)
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--runner", default="")
  parser.add_argument("--reference-runner", default="")
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  cases = scenarios()
  (args.output / "cases.json").write_text(json.dumps(cases, indent=2) + "\n")
  reference_cmd = shlex.split(args.reference_runner) + [str(args.reference)]
  native_cmd = shlex.split(args.runner) + [str(args.binary)]
  steps = 0
  for start in range(0, len(cases), 12):
    batch = cases[start : start + 12]
    commands = [command for case in batch for command in case["commands"]]
    source_input = "\n".join(map(source_line, commands)) + "\n"
    native_input = "\n".join(json.dumps(command) for command in commands) + "\n"
    expected = subprocess.run(reference_cmd, input=source_input, text=True, capture_output=True, timeout=90)
    actual = subprocess.run(native_cmd, input=native_input, text=True, capture_output=True, timeout=90)
    prefix = args.output / f"batch-{start:03d}"
    prefix.with_suffix(".source.jsonl").write_text(expected.stdout)
    prefix.with_suffix(".native.jsonl").write_text(actual.stdout)
    prefix.with_suffix(".stderr.json").write_text(json.dumps({"source": expected.stderr, "native": actual.stderr}) + "\n")
    if expected.returncode or actual.returncode:
      raise RuntimeError(f"batch {start}: source={expected.returncode}, native={actual.returncode}; see {prefix}")
    left = [json.loads(line) for line in expected.stdout.splitlines()]
    right = [json.loads(line) for line in actual.stdout.splitlines()]
    if len(left) != len(commands) or len(right) != len(commands):
      raise AssertionError(f"batch {start}: output count mismatch")
    for index, (command, source, native) in enumerate(zip(commands, left, right, strict=True)):
      if source != native:
        failure = {"batch": start, "index": index, "command": command, "source": source, "native": native}
        (args.output / "failure.json").write_text(json.dumps(failure, indent=2) + "\n")
        raise AssertionError(f"lifecycle mismatch at batch {start} index {index}: {command}")
    steps += sum(command["op"] == "step" for command in commands)
  report = {
    "status": "PASS",
    "scenarios": len(cases),
    "steps": steps,
    "source_command": reference_cmd,
    "native_command": native_cmd,
    "source_sha256": sha(args.reference),
    "native_sha256": sha(args.binary),
    "scope": "Original verbatim frame lifecycle methods; controlled clock, stress and fence/queue I/O boundaries; no device or ioctl ABI claim",
  }
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps(report))


if __name__ == "__main__":
  main()
