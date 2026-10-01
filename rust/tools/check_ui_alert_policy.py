# /// script
# dependencies = []
# ///
# How to run: python rust/tools/check_ui_alert_policy.py --binary <alert_policy> --output <evidence>
"""Execute original get_alert ASTs at all timeout gates; no renderer or device is simulated."""

import argparse
import ast
from dataclasses import asdict, dataclass
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace


class EnumValue(int):
  @property
  def raw(self) -> int:
    return int(self)


class Messages(dict):
  def __init__(self, value, item):
    super().__init__(selfdriveState=SimpleNamespace(
      alertText1=value['text1'], alertText2=value['text2'], alertSize=EnumValue(value['size']),
      alertStatus=EnumValue(value['status']), alertHudVisual=value['visual_alert'], alertType=value['alert_type'], enabled=item['enabled'],
    ))
    self.updated = {'selfdriveState': item['updated']}
    self.recv_frame = {'selfdriveState': item['receive_frame']}
    self.recv_time = {'selfdriveState': item['receive_time']}


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  current = {'text1': 'Original current', 'text2': 'Current subtitle', 'size': 2, 'status': 1, 'visual_alert': 0, 'alert_type': ''}
  cases = []
  for compact, tici, updated, recv, enabled, now, size in itertools.product(
    [False, True], [False, True], [False, True], [9, 10, 11], [False, True],
    [0.0, 4.999999999, 5.0, 5.000000001, 14.999999999, 15.0, 15.000000001, 30.0], [0, 1, 2, 3],
  ):
    cases.append({'compact': compact, 'inputs': [{
      'current': dict(current, size=size), 'enabled': enabled, 'updated': updated,
      'receive_frame': recv, 'receive_time': 0.0, 'started_frame': 10, 'started_time': 0.0, 'now': now, 'tici': tici,
    }]})
  fallback = dict(current, text1='openpilot Unavailable', text2='Waiting to start', size=2, status=0)
  for previous in [False, True]:
    sequence = []
    if previous:
      sequence.append({'current': current, 'enabled': False, 'updated': True, 'receive_frame': 11,
                       'receive_time': 0.0, 'started_frame': 10, 'started_time': 0.0, 'now': 0.0, 'tici': True})
    sequence.append({'current': fallback, 'enabled': False, 'updated': False, 'receive_frame': 9,
                     'receive_time': 0.0, 'started_frame': 10, 'started_time': 0.0, 'now': 6.0, 'tici': True})
    sequence.append(dict(sequence[-1], now=6.1, updated=True, current=dict(current, size=0)))
    cases.append({'compact': True, 'inputs': sequence})
  expected = []
  sources = []
  for case in cases:
    source_path = root / ('openpilot/selfdrive/ui/mici/onroad/alert_renderer.py' if case['compact'] else 'openpilot/selfdrive/ui/onroad/alert_renderer.py')
    source = source_path.read_text()
    tree = ast.parse(source)
    names = {'SELFDRIVE_STATE_TIMEOUT', 'SELFDRIVE_UNRESPONSIVE_TIMEOUT', 'ALERT_STARTUP_PENDING', 'ALERT_CRITICAL_TIMEOUT', 'ALERT_CRITICAL_REBOOT'}
    nodes = []
    for node in tree.body:
      if isinstance(node, ast.ClassDef) and node.name == 'Alert':
        nodes.append(node)
      if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in names for target in node.targets):
        nodes.append(node)
      if isinstance(node, ast.ClassDef) and node.name == 'AlertRenderer':
        nodes.append(ast.ClassDef(name='Renderer', bases=[], keywords=[], body=[
          item for item in node.body if isinstance(item, ast.FunctionDef) and item.name == 'get_alert'
        ], decorator_list=[]))
    env = {'dataclass': dataclass, 'AlertSize': SimpleNamespace(mid=2, full=3), 'AlertStatus': SimpleNamespace(normal=0, critical=2),
           'car': SimpleNamespace(CarControl=SimpleNamespace(HUDControl=SimpleNamespace(VisualAlert=SimpleNamespace(none=0)))),
           'messaging': SimpleNamespace(SubMaster=Messages), 'tr': lambda text: text, 'TICI': False,
           'time': SimpleNamespace(monotonic=lambda: 0.0), 'ui_state': SimpleNamespace(started_time=0.0, started_frame=0)}
    exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(source_path), 'exec'), env)
    renderer = env['Renderer']()
    renderer._prev_alert = None
    rows = []
    for item in case['inputs']:
      env['TICI'] = item['tici']
      env['time'].monotonic = lambda item=item: item['now']
      env['ui_state'].started_time = item['started_time']
      env['ui_state'].started_frame = item['started_frame']
      alert = renderer.get_alert(Messages(item['current'], item))
      value = asdict(alert) if alert is not None else None
      if value is not None and not case['compact']:
        value.update(visual_alert=0, alert_type='')
      previous = asdict(renderer._prev_alert) if renderer._prev_alert is not None else None
      rows.append({'current': value, 'previous': previous})
    expected.append(rows)
    sources.append({'path': str(source_path.relative_to(root)), 'sha256': hashlib.sha256(source.encode()).hexdigest()})
  fixture = json.dumps(cases)
  (args.output / 'input.json').write_text(fixture)
  (args.output / 'source.json').write_text(json.dumps(expected, indent=2))
  result = subprocess.run([str(args.binary)], input=fixture, text=True, capture_output=True, check=True)
  (args.output / 'native.json').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr)
  actual = json.loads(result.stdout)
  assert actual == expected, next((i, cases[i], a, e) for i, (a, e) in enumerate(zip(actual, expected, strict=True)) if a != e)
  summary = {'cases': len(cases), 'steps': sum(len(case['inputs']) for case in cases), 'different_results': 0,
             'sources': list({source['path']: source for source in sources}.values())}
  (args.output / 'results.json').write_text(json.dumps(summary, indent=2))
  print(f'PASS: {summary["cases"]} source/native alert policy cases, {summary["steps"]} steps, exact current/previous selection')


if __name__ == '__main__':
  main()
