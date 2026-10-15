"""Final production dispatcher prerequisites, before tracker/model construction.

AST prefixes are unchanged; only the expensive leaf is replaced by a route tag.
The neural-disabled color fallback is a leaf: its tracker reads are separate.
"""
import argparse
import ast
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType
from signal_source import source, compare


def prefix(path, name, stop):
  node = next(n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name == name)
  node.body = node.body[:next(i for i, n in enumerate(node.body) if stop(n))]
  node.body.append(ast.Return(value=ast.Constant('cpu')))
  return ast.fix_missing_locations(ast.Module(body=[node], type_ignores=[]))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  neural_path = 'openpilot/selfdrive/modeld/signal_neural_shadow.py'
  color_path = 'openpilot/selfdrive/modeld/signal_color_shadow.py'
  neural_ast = prefix(neural_path, 'run', lambda n: isinstance(n, ast.Import) and n.names[0].name == 'fcntl')
  color_ast = prefix(color_path, 'run', lambda n: isinstance(n, ast.ImportFrom) and n.module.endswith('signal_tracking_shadow'))
  # Non-neural fallback is intentionally a leaf, not the legacy analysis owner.
  color_ast.body[0].body[-1] = ast.Return(value=ast.Constant('classical'))
  ast.fix_missing_locations(color_ast)
  cases, expected = [], []
  mode_module = ModuleType('openpilot.selfdrive.carrot.signal_mode')
  neural_module = ModuleType('openpilot.selfdrive.modeld.signal_neural_shadow')
  tracking_module = ModuleType('openpilot.selfdrive.modeld.signal_tracking_shadow')
  for module in (mode_module, neural_module, tracking_module):
    sys.modules[module.__name__] = module
  class ReferencePath(type(Path())):
    def mkdir(self, *args, **kwargs):
      pass  # IO double only; do not create production directories.
  directory = ReferencePath('/data/signal-color-shadow')
  for initial, enabled, first, second, backend in itertools.product(range(3), (False, True), ('0', '1', 'error'), ('0', '1', 'error'), ('cpu', 'auto', 'qcom', 'invalid')):
    case = dict(initial=initial, enabled=enabled, flags=[first, second], backend=backend)
    trace = []
    flags = iter(case['flags'])
    def mode():
      trace.append('mode')
      return initial
    def requested(_directory):
      trace.append('requested')
      return enabled
    def neural_requested(_directory):
      trace.append('neural')
      value = next(flags)
      if value == 'error':
        raise UnicodeError('fixture decode failure')
      return value == '1'
    def selection(_directory):
      trace.append('backend')
      if backend == 'invalid':
        raise ValueError('invalid neural backend selection')
      return backend
    def automatic(*_):
      return 'automatic'
    neural_scope = dict(Path=ReferencePath, DIRECTORY=directory, requested=requested, neural_requested=neural_requested, backend_requested=selection, run_auto=automatic)
    exec(compile(neural_ast, 'e8e152db:' + neural_path, 'exec'), neural_scope)
    mode_module.signal_mode = mode
    neural_module.neural_requested = neural_requested
    neural_module.run = neural_scope['run']
    tracking_module.run = lambda *_args, **_kwargs: 'classical'
    scope = dict(Path=ReferencePath, DIRECTORY=directory)
    exec(compile(color_ast, 'e8e152db:' + color_path, 'exec'), scope)
    try:
      route = scope['run']()
      result = dict(route=route or 'disabled', trace=trace)
    except (UnicodeError, ValueError):
      result = dict(route='error', trace=trace)
    cases.append(case)
    expected.append(result)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(cases=len(cases), reads=sum(len(v['trace']) for v in expected), native_compared=bool(args.binary))))


if __name__ == '__main__':
  main()
