"""Actual unchanged final worker warmup loop; not a complete worker oracle."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import numpy as np
from signal_source import source


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  path = 'openpilot/selfdrive/modeld/signal_neural_shadow.py'
  run = next(n for n in ast.parse(source(path)).body if isinstance(n, ast.FunctionDef) and n.name == 'run')
  loops = [n for n in ast.walk(run) if isinstance(n, ast.For) and ast.unparse(n.iter) == 'range(3)']
  assert len(loops) == 1
  calls = []
  class Session:
    def __init__(self, name):
      self.name = name
    def run(self, outputs, inputs):
      assert outputs is None
      name, array = next(iter(inputs.items()))
      calls.append(dict(network=self.name, name=name, shape=list(array.shape), sha256=hashlib.sha256(array.tobytes()).hexdigest()))
  scope = dict(np=np, network=SimpleNamespace(detector=Session('detector'), classifier=Session('classifier')))
  exec(compile(ast.Module(loops, type_ignores=[]), path, 'exec'), scope)
  (args.output / 'expected.json').write_text(json.dumps(calls))
  print(json.dumps(dict(calls=len(calls), scope='actual source warmup loop; scripted session boundary')))


if __name__ == '__main__':
  main()
