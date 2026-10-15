"""Exact final classical record/publication statements with phase-mode changes.

Pixel processing is frozen separately. Actual source AST statements and payload
writers execute; only the four production mode checks and logging are injected.
"""
import argparse
import ast
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType, SimpleNamespace
from check_signal_publication import publisher
from signal_source import source, compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  path = 'openpilot/selfdrive/modeld/signal_tracking_shadow.py'
  run = next(n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name == 'run')
  dictionary = next(n for n in ast.walk(run) if isinstance(n, ast.Dict) and any(isinstance(k, ast.Constant) and k.value == 'start_guard_transport' for k in n.keys))
  field = next(v for k, v in zip(dictionary.keys, dictionary.values) if isinstance(k, ast.Constant) and k.value == 'start_guard_transport')
  statements = []
  for node in ast.walk(run):
    if isinstance(node, ast.Try) and any(isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id in ('publish_start_observation', 'publish_prepare_observation') for n in node.body for n in ast.walk(n)):
      statements.append(node)
    if isinstance(node, ast.If) and isinstance(node.test, ast.Call) and isinstance(node.test.func, ast.Name) and node.test.func.id == 'display_requested':
      statements.append(node)
  assert len(statements) == 3
  statements.sort(key=lambda node: node.lineno)
  start = publisher('openpilot/selfdrive/carrot/signal_start_guard.py', 'publish_start_observation')
  prepare = publisher('openpilot/selfdrive/carrot/signal_stop_prepare_runtime.py', 'publish_prepare_observation')
  display = publisher('openpilot/selfdrive/carrot/signal_display.py', 'publish_display')
  display_node = next(n for n in ast.parse(source('openpilot/selfdrive/carrot/signal_display.py')).body if isinstance(n, ast.FunctionDef) and n.name == 'display_requested')
  display_scope = dict(Path=Path, DISPLAY_FLAG=Path('/data/signal-color-shadow/display_enabled'))
  exec(compile(ast.Module(body=[display_node], type_ignores=[]), 'e8e152db:display_requested', 'exec'), display_scope)
  cases, expected = [], []
  for modes in itertools.product((False, True), repeat=4):
    for malformed in (False, True):
      case = dict(modes=modes, malformed=malformed)
      calls, events = [], []
      def enabled(path):
        value = modes[len(calls)]
        calls.append(value)
        return value
      module = ModuleType('openpilot.selfdrive.carrot.signal_mode')
      module.flag_override = enabled
      sys.modules[module.__name__] = module
      output = args.output / str(len(cases)); output.mkdir()
      def bind(function, name):
        function.__globals__['requested'] = enabled
        function.__globals__['ENABLE_PATH'] = Path('/data/signal-color-shadow') / ('start_guard_enabled' if name == 'start' else 'stop_prepare_enabled')
        return lambda *a, **k: function(*a, **k, path=output / (name + '.json'))
      record = dict(timestamp_eof=10_000_000_000, frame_id=20, session='phase', revalidated=True,
                    tracks=None if malformed else [dict(id=1, box=[600., 200., 640., 220.], state='red', age=0., observations=3, seen_red=True, evidence=dict(raw='red'))])
      scope = dict(directory=Path('/data/signal-color-shadow'), flag_requested=enabled)
      record['start_guard_transport'] = eval(compile(ast.Expression(body=field), 'e8e152db:record_flag', 'eval'), scope)
      scope.update(record=record, comparison_record=None,
                   publish_start_observation=bind(start, 'start'), publish_prepare_observation=bind(prepare, 'prepare'),
                   publish_display=bind(display, 'display'), display_requested=display_scope['display_requested'],
                   cloudlog=SimpleNamespace(exception=lambda name: events.append(name)))
      exec(compile(ast.Module(body=statements, type_ignores=[]), 'e8e152db:publication_phases', 'exec'), scope)
      assert len(calls) == 4
      payloads = {name: json.loads((output / (name + '.json')).read_text()) if (output / (name + '.json')).exists() else None for name in ('start', 'prepare', 'display')}
      expected.append(dict(record_flag=record['start_guard_transport'], calls=calls, events=events, payloads=payloads))
      cases.append(case)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(cases=len(cases), mode_reads=sum(len(v['calls']) for v in expected), native_compared=bool(args.binary))))


if __name__ == '__main__':
  main()
