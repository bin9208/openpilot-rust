import argparse
import hashlib
import json
from pathlib import Path
import random
import subprocess
from typing import TypedDict


class Camera(TypedDict):
  frame_id: int
  integration_lines: int
  mono_time_ns: int


class Step(TypedDict):
  frame: int
  now_ns: int
  fan_speed: int | None
  camera: Camera | None
  fan_control: bool
  driver_view: bool


def cases() -> list[list[Step]]:
  scenarios: list[list[Step]] = []
  for driver_view in [False, True]:
    for lines in [0, -1, -2147483648, 399, 400, 401, 601, 999, 1000, 1001, 60660, 102000, 2147483647]:
      rows: list[Step] = []
      for frame in range(240):
        now = frame * 50_000_000
        rows.append({'frame': frame + 1, 'now_ns': now, 'fan_speed': 40, 'fan_control': True,
                     'driver_view': driver_view, 'camera': {'frame_id': frame, 'integration_lines': lines, 'mono_time_ns': now}})
      scenarios.append(rows)
  rng = random.Random(175)
  for seed in range(12):
    rows = []
    camera_id = 0
    for frame in range(2000):
      now = frame * 50_000_000
      if frame % 113 == 0:
        camera_id = 0
      if frame % 193 == 0:
        camera_id = 4294967295
      camera: Camera = {'frame_id': camera_id, 'integration_lines': rng.choice([-200, 0, 400, 1000, 2309, 60660, 102000]),
                        'mono_time_ns': max(0, now - rng.choice([0, 1_000_000_000, 1_000_000_001, -1]))}
      camera_id = (camera_id + 1) % 4294967296
      rows.append({'frame': frame + 1, 'now_ns': now, 'fan_speed': rng.choice([None, 0, 40, 999, 65535]),
                   'fan_control': frame % 37 != 0, 'camera': camera if frame % 3 else None,
                   'driver_view': (frame + seed) % 17 < 8})
    scenarios.append(rows)
  return scenarios


def main() -> None:
  parser = argparse.ArgumentParser(description='Compare all native fan/IR/Params effects with original Panda peripheral policy.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  output: Path = args.output
  output.mkdir(parents=True, exist_ok=False)
  scenarios = cases()
  count = 0
  for index, steps in enumerate(scenarios):
    payload = json.dumps(steps) + '\n'
    (output / f'{index}-input.json').write_text(payload)
    observed = []
    for name, command in [('source', [str(args.source)]), ('native', [*args.runner, str(args.native)])]:
      run = subprocess.run(command, input=payload, text=True, capture_output=True, check=False, timeout=30)
      (output / f'{index}-{name}.json').write_text(run.stdout)
      (output / f'{index}-{name}.stderr').write_text(run.stderr)
      run.check_returncode()
      observed.append(json.loads(run.stdout))
    if observed[0] != observed[1]:
      differences = [i for i, (source, native) in enumerate(zip(*observed, strict=True)) if source != native]
      raise AssertionError(f'Peripheral scenario {index} differs at steps {differences[:12]}')
    if len(observed[0]) != len(steps):
      raise AssertionError(f'Incomplete peripheral trace in scenario {index}')
    count += len(steps)
  report = {'status': 'PASS', 'scenarios': len(scenarios), 'steps': count,
            'scope': 'exact ordered fan/IR commands and conditional driver-view Params reads',
            'source_sha256': hashlib.sha256(args.source.read_bytes()).hexdigest(),
            'native_sha256': hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
