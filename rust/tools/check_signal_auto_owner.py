"""Finite traces from exact final auto-owner source, with explicit fake process IO.

The real process terminate/kill/join boundary is separately exercised by
signal_runtime_trace. These cases compare source sequencing and failure cleanup.
"""
import argparse
import ast
import itertools
import json
from pathlib import Path
import sys
import subprocess
from types import ModuleType, SimpleNamespace
from signal_source import source, compare


def reference(case):
  actions = []
  tick = 0
  created = 0

  class Child:
    def __init__(self, target, args, daemon):
      nonlocal created
      assert daemon and target is selected and args[0] == 'fixture'
      self.backend = args[1]
      self.pid = 100 + created
      created += 1
      self.alive = True
      actions.append(['create', self.backend, self.pid])

    def start(self):
      actions.append(['start', self.pid])

    def is_alive(self):
      actions.append(['alive', self.pid, self.alive])
      return self.alive

    def terminate(self):
      actions.append(['terminate', self.pid])
      if not case['stubborn']:
        self.alive = False

    def join(self, timeout):
      actions.append(['join', self.pid, timeout])

    def kill(self):
      actions.append(['kill', self.pid])
      self.alive = False

    def close(self):
      actions.append(['close', self.pid])

    @property
    def exitcode(self):
      value = 7 if case['fault'] and tick == 1 else None
      actions.append(['exitcode', self.pid, value])
      return value

  def selected(*args):
    raise AssertionError('fake process must never execute target')

  def requested(directory):
    assert directory == 'fixture'
    value = tick < len(case['modes'])
    actions.append(['requested', value])
    return value

  def neural(directory):
    actions.append(['neural', True])
    return True

  def backend(directory):
    actions.append(['selection', 'auto'])
    return 'auto'

  def automatic(params):
    if case['read_error'] and tick == 1:
      actions.append(['automatic_error'])
      raise OSError('source params read failed')
    value = case['modes'][tick]
    actions.append(['automatic', value])
    return value

  def sleep(seconds):
    nonlocal tick
    actions.append(['sleep', seconds])
    tick += 1

  def install(name, **members):
    module = ModuleType(name)
    module.__dict__.update(members)
    sys.modules[name] = module

  install('multiprocessing', get_context=lambda method: SimpleNamespace(Process=Child))
  install('signal', SIGTERM=15, signal=lambda signum, callback: 'previous-handler')
  install('openpilot.common.params', Params=lambda: object())
  install('openpilot.common.swaglog', cloudlog=SimpleNamespace(
    event=lambda name, **fields: actions.append(['event', name, fields])))
  path = 'openpilot/selfdrive/modeld/signal_neural_shadow.py'
  node = next(n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name == 'run_auto')
  scope = dict(DIRECTORY='fixture')
  exec(compile(ast.Module(body=[node], type_ignores=[]), 'e8e152db:' + path, 'exec'), scope)
  scope.update(DIRECTORY='fixture', backend_requested=backend, requested=requested,
               neural_requested=neural, automatic_backend=automatic, run_selected=selected,
               time=SimpleNamespace(monotonic=lambda: tick * .1, sleep=sleep))
  try:
    scope['run_auto']('fixture')
  except (RuntimeError, OSError) as error:
    actions.append(['error', str(error)])
  return actions


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cases = []
  for length in range(5):
    for modes in itertools.product(('tracking_cpu', 'qcom'), repeat=length):
      for stubborn in (False, True):
        cases.append(dict(modes=modes, stubborn=stubborn, fault=False, read_error=False))
  for backend in ('tracking_cpu', 'qcom'):
    for stubborn in (False, True):
      for fault in ('fault', 'read_error'):
        cases.append(dict(modes=[backend, backend], stubborn=stubborn,
                          fault=fault == 'fault', read_error=fault == 'read_error'))
  expected = [reference(case) for case in cases]
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(cases=len(cases), actions=sum(map(len, expected)), native_compared=bool(args.binary))))


if __name__ == '__main__':
  main()
