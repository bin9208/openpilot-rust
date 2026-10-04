# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
# Run with Python 3.12: check_selfdrive_state.py --binary TARGET/selfdrive_state --output EVIDENCE
import argparse
import ast
from contextlib import redirect_stdout
from dataclasses import dataclass
import hashlib
import io
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace
from typing import Final, TypedDict

ROOT: Final = Path(__file__).resolve().parents[2]
STATES: Final = ('disabled', 'preEnabled', 'enabled', 'softDisabling', 'overriding')
TYPES: Final = ('enable', 'preEnable', 'overrideLateral', 'overrideLongitudinal', 'noEntry',
               'warning', 'userDisable', 'softDisable', 'immediateDisable', 'permanent')


class Request(TypedDict):
  state: str | None
  timer: int | None
  events: list[str]


class Response(TypedDict):
  state: str
  soft_disable_timer: int
  current_alert_types: list[str]
  enabled: bool
  active: bool


@dataclass(frozen=True, slots=True)
class MaskEvents:
  events: tuple[str, ...]

  def contains(self, category: str) -> bool:
    return category in self.events


def cases() -> list[Request]:
  rows: list[Request] = []
  for state in STATES:
    for timer in (0, 1, 49, 50, 299, 300):
      for bits in range(1 << len(TYPES)):
        rows.append({'state': state, 'timer': timer, 'events': [value for index, value in enumerate(TYPES) if bits & (1 << index)]})
  rows.append({'state': 'enabled', 'timer': 0, 'events': []})
  rows.extend({'state': None, 'timer': None, 'events': ['softDisable']} for _ in range(302))
  rng = random.Random(168)
  for _ in range(20_000):
    rows.append({'state': None, 'timer': None, 'events': [value for value in TYPES if rng.random() < 0.12]})
  return rows


def original(rows: list[Request]) -> tuple[list[Response], str]:
  source_path = ROOT / 'openpilot/selfdrive/selfdrived/state.py'
  source = source_path.read_text()
  event_tree = ast.parse((ROOT / 'openpilot/selfdrive/selfdrived/events.py').read_text())
  event_type = next(node for node in event_tree.body if isinstance(node, ast.ClassDef) and node.name == 'ET')
  tree = ast.parse(source)
  retained = [node for node in tree.body if isinstance(node, ast.ClassDef) or (
    isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in (
      'SOFT_DISABLE_TIME', 'ACTIVE_STATES', 'ENABLED_STATES') for target in node.targets))]
  timing = ast.parse((ROOT / 'openpilot/common/realtime.py').read_text())
  dt_ctrl = next(ast.literal_eval(node.value) for node in timing.body if isinstance(node, ast.Assign)
                 and any(isinstance(target, ast.Name) and target.id == 'DT_CTRL' for target in node.targets))
  namespace = {'State': SimpleNamespace(**{state: state for state in STATES}), 'DT_CTRL': dt_ctrl, 'Events': MaskEvents}
  exec(compile(ast.Module(body=[event_type, *retained], type_ignores=[]), str(source_path), 'exec'), namespace)
  machine = namespace['StateMachine']()
  expected: list[Response] = []
  with redirect_stdout(io.StringIO()):
    for row in rows:
      if row['state'] is not None:
        machine.state = row['state']
      if row['timer'] is not None:
        machine.soft_disable_timer = row['timer']
      enabled, active = machine.update(MaskEvents(tuple(row['events'])))
      expected.append({'state': machine.state, 'soft_disable_timer': machine.soft_disable_timer,
                       'current_alert_types': machine.current_alert_types.copy(), 'enabled': enabled, 'active': active})
  return expected, hashlib.sha256(source.encode()).hexdigest()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  rows = cases()
  expected, source_hash = original(rows)
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  (args.output / 'source.json').write_text(json.dumps(expected))
  process = subprocess.run([str(args.binary.resolve())], input=payload, text=True, capture_output=True, timeout=30)
  (args.output / 'native.stdout').write_text(process.stdout)
  (args.output / 'native.stderr').write_text(process.stderr)
  process.check_returncode()
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  assert len(actual) == len(expected)
  for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
    assert left == right, (index, rows[index], left, right)
  result = {'pass': True, 'steps': len(rows), 'exhaustive_single_steps': 5 * 6 * 1024,
            'source_sha256': source_hash, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))


if __name__ == '__main__':
  main()
