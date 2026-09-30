"""Execute the unchanged save_bootlog with original Params and real thread/process calls."""

import argparse
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
from types import SimpleNamespace

from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binding', type=Path)
  parser.add_argument('basedir', type=Path)
  parser.add_argument('mode')
  args = parser.parse_args()
  params, _ = load(args.binding, 'ipc://' + str(args.basedir / 'swaglog'), args.basedir / 'swaglog-files')
  workers = []
  outcomes = []

  def thread(*positional, **keywords):
    worker = threading.Thread(*positional, **keywords)
    workers.append(worker)
    return worker

  threading.excepthook = lambda args: (outcomes.append(False), threading.__excepthook__(args))
  namespace = {
    'tempfile': tempfile,
    'pathlib': __import__('pathlib'),
    'Params': params.Params,
    'os': os,
    'shutil': shutil,
    'subprocess': subprocess,
    'threading': SimpleNamespace(Thread=thread),
    'BASEDIR': str(args.basedir),
  }
  source = ROOT / 'openpilot/system/manager/helpers.py'
  tree = ast.parse(source.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'save_bootlog']
  exec(compile(tree, str(source), 'exec'), namespace)
  try:
    namespace['save_bootlog']()
  except Exception:
    print(json.dumps({'phase': 'copy_failed'}), flush=True)
    return
  assert workers[0].daemon is True
  print(json.dumps({'phase': 'returned'}), flush=True)
  if args.mode == 'detached':
    return
  workers[0].join()
  print(json.dumps({'phase': 'worker_done', 'success': not outcomes}), flush=True)


if __name__ == '__main__':
  main()
