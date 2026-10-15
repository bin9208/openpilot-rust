"""Actual final automatic_backend Params ordering, short circuits and failures."""
import argparse
import ast
import itertools
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType
from signal_source import classes, source, compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  read = classes('openpilot/selfdrive/carrot/signal_mode.py', ['read_mode'])
  read['PARAM'] = 'SignalAssistMode'
  module = ModuleType('openpilot.selfdrive.carrot.signal_mode')
  module.read_mode = read['read_mode']
  sys.modules[module.__name__] = module
  path = 'openpilot/selfdrive/modeld/signal_neural_shadow.py'
  node = next(n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name == 'automatic_backend')
  scope = {}
  exec(compile(ast.Module(body=[node], type_ignores=[]), 'e8e152db:' + path, 'exec'), scope)
  keys = ['DriverMonitoringEnabled', 'DriverMonitoringSessionDisabled', 'IsDriverViewEnabled', 'UsbGpuActive']
  cases, expected = [], []
  for mode, *flags in itertools.product(range(-1, 4), *[range(2)] * 4):
    for failure in [None, 'SignalAssistMode'] + keys:
      calls = []
      values = dict(zip(keys, map(bool, flags)))
      class Params:
        def get_int(self, key):
          calls.append(['int', key])
          if failure == key:
            raise OSError('read:' + key)
          return mode
        def get_bool(self, key):
          calls.append(['bool', key])
          if failure == key:
            raise OSError('read:' + key)
          return values[key]
      case = dict(mode=mode, values=values, failure=failure)
      try:
        result = dict(backend=scope['automatic_backend'](Params()))
      except OSError as error:
        result = dict(error=str(error))
      result['calls'] = calls
      cases.append(case)
      expected.append(result)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(cases=len(cases), reads=sum(len(v['calls']) for v in expected), native_compared=bool(args.binary))))


if __name__ == '__main__':
  main()
