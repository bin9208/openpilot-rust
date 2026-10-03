import argparse
import ast
from collections import defaultdict
from dataclasses import dataclass
from enum import IntEnum
import hashlib
import json
from pathlib import Path
import random
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PRIORITIES = ('lowest', 'lower', 'low', 'mid', 'high', 'highest')
CATEGORIES = ('enable', 'preEnable', 'overrideLateral', 'overrideLongitudinal', 'noEntry',
              'warning', 'userDisable', 'softDisable', 'immediateDisable', 'permanent')


def source_classes():
  from openpilot.cereal import log, car
  events_path = ROOT / 'openpilot/selfdrive/selfdrived/events.py'
  manager_path = ROOT / 'openpilot/selfdrive/selfdrived/alertmanager.py'
  timing_path = ROOT / 'openpilot/common/realtime.py'
  timing = ast.parse(timing_path.read_text())
  dt = next(ast.literal_eval(node.value) for node in timing.body if isinstance(node, ast.Assign)
            and any(isinstance(target, ast.Name) and target.id == 'DT_CTRL' for target in node.targets))
  namespace = {'IntEnum': IntEnum, 'dataclass': dataclass, 'defaultdict': defaultdict, 'DT_CTRL': dt,
               'log': log, 'car': car, 'AlertStatus': log.SelfdriveState.AlertStatus,
               'AlertSize': log.SelfdriveState.AlertSize, 'VisualAlert': car.CarControl.HUDControl.VisualAlert,
               'AudibleAlert': car.CarControl.HUDControl.AudibleAlert}
  nodes = [node for node in ast.parse(events_path.read_text()).body if (
    isinstance(node, ast.ClassDef) and node.name in ('Alert', 'Priority')) or (
    isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'EmptyAlert' for target in node.targets))]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(events_path), 'exec'), namespace)
  nodes = [node for node in ast.parse(manager_path.read_text()).body if isinstance(node, ast.ClassDef)]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(manager_path), 'exec'), namespace)
  return namespace, {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                     for path in (events_path, manager_path, timing_path)}


def serialize(alert):
  result = vars(alert).copy()
  result['priority'] = PRIORITIES[int(alert.priority)]
  return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  namespace, sources = source_classes()
  machine = namespace['AlertManager']()
  rng = random.Random(168)
  rows, expected = [], []
  frame = 0
  for index in range(25_000):
    frame += rng.choice((0, 1, 1, 1, 2, 10))
    added = []
    for _ in range(rng.randrange(4)):
      priority = rng.randrange(6)
      item = namespace['Alert'](f'text-{index}', '운전 경고', rng.randrange(3), rng.randrange(4),
                                priority, 0, 0, rng.choice((0., .01, .2, 1., 3.)))
      item.event_type = rng.choice(CATEGORIES)
      item.alert_type = str(rng.randrange(15))
      added.append(item)
    clear = rng.sample(CATEGORIES, rng.randrange(3))
    machine.add_many(frame, added)
    machine.process_alerts(frame, set(clear))
    rows.append({'frame': frame, 'add': [serialize(item) for item in added], 'clear': clear})
    entries = [{**vars(entry), 'alert': serialize(entry.alert)} for entry in machine.alerts.values()]
    expected.append({'current': serialize(machine.current_alert), 'entries': entries})
  payload = ''.join(json.dumps(row) + '\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  result = subprocess.run([str(args.binary.resolve())], input=payload, text=True, capture_output=True, timeout=60)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
  assert result.returncode == 0, result.stderr
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert len(actual) == len(expected)
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    assert source == native, (index, source, native)
  report = {'result': 'PASS', 'steps': len(rows), 'sources': sources,
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
