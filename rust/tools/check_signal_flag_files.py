"""Actual final flag/backend file predicates, including decoding failures."""
import argparse
import ast
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType
from signal_source import source, compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  mode = ModuleType('openpilot.selfdrive.carrot.signal_mode')
  mode.flag_override = lambda path: None  # isolated non-production paths
  sys.modules[mode.__name__] = mode
  scope = dict(Path=Path, DIRECTORY=Path('/data/signal-color-shadow'))
  for path, names in [
    ('openpilot/selfdrive/modeld/signal_color_shadow.py', ['requested']),
    ('openpilot/selfdrive/modeld/signal_neural_shadow.py', ['backend_requested']),
  ]:
    nodes = [n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name in names]
    exec(compile(ast.Module(body=nodes, type_ignores=[]), 'e8e152db:' + path, 'exec'), scope)
  values = [b'', b'0', b'1', b'01', b'+1', b'true', b'cpu', b'qcom', b'auto', b'unknown', b'\xff', b'1\xff', b'\0']
  for text in ('1', 'cpu', 'qcom', 'auto'):
    values.extend((space + text + space).encode() for space in (' ', '\t\n', '\x1c\x1f', '\u0085', '\u2000', '\u3000'))
  cases = [dict(kind='missing', data=[]), dict(kind='directory', data=[])] + [dict(kind='value', data=list(v)) for v in values]
  expected = []
  with tempfile.TemporaryDirectory(prefix='signal-flag-source-') as temporary:
    for index, case in enumerate(cases):
      directory = Path(temporary) / str(index)
      directory.mkdir()
      for name in ('enabled', 'neural_backend'):
        path = directory / name
        if case['kind'] == 'directory':
          path.mkdir()
        elif case['kind'] == 'value':
          path.write_bytes(bytes(case['data']))
      record = {}
      for name, function in [('flag', scope['requested']), ('backend', scope['backend_requested'])]:
        try:
          record[name] = dict(value=function(directory))
        except (OSError, UnicodeError, ValueError):
          record[name] = dict(error=True)
      expected.append(record)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(cases=len(cases), native_compared=bool(args.binary))))


if __name__ == '__main__':
  main()
