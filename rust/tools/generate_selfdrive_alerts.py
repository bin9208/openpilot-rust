import argparse
import ast
import bisect
import copy
from enum import IntEnum
import hashlib
import json
import math
import os
from pathlib import Path
from types import SimpleNamespace

from openpilot.cereal import car, log

ROOT = Path(__file__).resolve().parents[2]
PRIORITIES = ('lowest', 'lower', 'low', 'mid', 'high', 'highest')


def load_source(device):
  source = ROOT / 'openpilot/selfdrive/selfdrived/events.py'
  timing = ROOT / 'openpilot/common/realtime.py'
  dt = next(ast.literal_eval(node.value) for node in ast.parse(timing.read_text()).body if isinstance(node, ast.Assign)
            and any(isinstance(target, ast.Name) and target.id == 'DT_CTRL' for target in node.targets))
  nodes = []
  for node in ast.parse(source.read_text()).body:
    if isinstance(node, (ast.Import, ast.ImportFrom)):
      continue
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'AlertCallbackType' for target in node.targets):
      continue
    if isinstance(node, ast.If) and ast.unparse(node.test) == "__name__ == '__main__'":
      continue
    nodes.append(node)
  namespace = {'car': car, 'log': log, 'IntEnum': IntEnum, 'bisect': bisect, 'copy': copy, 'math': math, 'os': os,
               'DT_CTRL': dt, 'HARDWARE': SimpleNamespace(get_device_type=lambda: device),
               'tr_noop': lambda text: text, 'tr': lambda text: text}
  annotations = ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)
  tree = ast.fix_missing_locations(ast.Module(body=[annotations, *nodes], type_ignores=[]))
  exec(compile(tree, str(source), 'exec'), namespace)
  return namespace


def serialize_alert(alert):
  result = vars(alert).copy()
  result['priority'] = PRIORITIES[int(alert.priority)]
  return result


def catalog(device):
  source = load_source(device)
  result = []
  for event, categories in source['EVENTS'].items():
    definitions = []
    for category, alert in categories.items():
      if isinstance(alert, source['Alert']):
        definition = {'kind': 'static', 'alert': serialize_alert(alert)}
      elif alert.__name__ == 'func':
        factory = alert.__qualname__.split('.')[0]
        assert factory in ('soft_disable_alert', 'user_soft_disable_alert'), factory
        closure = dict(zip(alert.__code__.co_freevars, (cell.cell_contents for cell in alert.__closure__), strict=True))
        assert set(closure) == {'alert_text_2'}, closure
        definition = {'kind': 'callback', 'callback': {'name': factory, 'text': closure['alert_text_2']}}
      else:
        definition = {'kind': 'callback', 'callback': {'name': alert.__name__}}
      definitions.append({'category': category, 'definition': definition})
    result.append({'event': event, 'name': source['EVENT_NAME'][event], 'definitions': definitions})
  return result


def generate():
  paths = ['openpilot/selfdrive/selfdrived/events.py', 'openpilot/common/realtime.py',
           'openpilot/cereal/log.capnp', 'opendbc_repo/opendbc/car/car.capnp']
  return {'sources': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths},
          'tici': catalog('tici'), 'mici': catalog('mici')}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--check', action='store_true')
  args = parser.parse_args()
  content = json.dumps(generate(), ensure_ascii=False, indent=2) + '\n'
  if args.check:
    assert args.output.read_text() == content, 'generated alert data differs from current source'
  else:
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(content)
  print(json.dumps({'result': 'PASS', 'output': str(args.output), 'sha256': hashlib.sha256(content.encode()).hexdigest()}))


if __name__ == '__main__':
  main()
