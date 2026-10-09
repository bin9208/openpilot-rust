#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_system_source.py < INPUT_JSON
from __future__ import annotations

import ast
from copy import deepcopy
import datetime
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import threading
import types
from typing import Final, assert_never

ROOT: Final = Path(__file__).resolve().parents[2]
BASE: Final = ROOT / 'openpilot/selfdrive/carrot/server'


def definitions(filename: str) -> types.ModuleType:
  path = BASE / filename
  tree = ast.parse(path.read_text(), filename=str(path))
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Assign, ast.AnnAssign))]
  tree.body.insert(0, ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0))
  ast.fix_missing_locations(tree)
  module = types.ModuleType(filename)
  module.__dict__.update({'math': math, 'threading': threading, 'time': types.SimpleNamespace(monotonic=lambda: 0.0), 'deepcopy': deepcopy})
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  return module


class Paths:
  def __init__(self, root: Path) -> None:
    self.root = root

  def owned(self, path: str) -> str:
    if path == '/data/etc/localtime':
      return str(self.root / 'localtime')
    if path.startswith('/usr/share/zoneinfo/'):
      return str(self.root / 'zones') + path[len('/usr/share/zoneinfo') :]
    return path

  def exists(self, path: str) -> bool:
    return os.path.exists(self.owned(path))

  def islink(self, path: str) -> bool:
    return os.path.islink(self.owned(path))

  def realpath(self, path: str) -> str:
    return os.path.realpath(self.owned(path)).replace(str(self.root / 'zones'), '/usr/share/zoneinfo')

  def getsize(self, path: str) -> int:
    return os.path.getsize(self.owned(path))


def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  match config['mode']:
    case 'time':
      module = definitions('services/time_sync.py')
      paths = Paths(root)
      module.os = types.SimpleNamespace(path=paths)
      module.datetime = datetime
      module.time = types.SimpleNamespace(time=lambda: config['now'])
      traces: list = []

      def run(command: list[str] | str, **kwargs):
        traces.append(command)
        if config.get('fail') == len(traces):
          raise subprocess.CalledProcessError(config.get('code', 1), command)
        if config.get('spawn_fail') == len(traces):
          raise FileNotFoundError(2, 'No such file or directory', 'sudo')
        match command:
          case ['sudo', 'rm', '-f', path]:
            Path(paths.owned(path)).unlink(missing_ok=True)
          case ['sudo', 'ln', '-s', source, destination]:
            os.symlink(paths.owned(source), paths.owned(destination))
          case str():
            assert kwargs == {'shell': True, 'check': True}
          case unexpected:
            assert_never(unexpected)
        return subprocess.CompletedProcess(command, 0)

      module.subprocess = types.SimpleNamespace(run=run, CalledProcessError=subprocess.CalledProcessError)
      try:
        value = module.sync_system_time_from_browser(config['body']['epoch_ms'], config['body'].get('timezone', 'UTC'))
      except (OSError, ValueError, OverflowError) as error:
        value = {'exception': str(error)}
      result = {'value': value, 'commands': traces}
    case 'network':
      module = definitions('services/device_info.py')
      module.subprocess = subprocess
      original_run = subprocess.run
      module.subprocess = types.SimpleNamespace(run=lambda args, **kwargs: original_run([str(root / 'nmcli'), *args[1:]], **kwargs))
      memory = {}
      module.get_param_value = lambda key, default: memory.get(key, default)
      values = []
      for step in config['steps']:
        memory.update({key: str(value) for key, value in step.get('values', {}).items()})
        match step['operation']:
          case 'refresh':
            values.append(module.refresh_device_network())
          case 'snapshot':
            values.append(module.get_device_network_snapshot())
          case unexpected:
            assert_never(unexpected)
      result = {'values': values}
    case 'select':
      module = definitions('features/system.py')
      result = {'values': [module._is_carrot_default_reset_param(row['name'], row['definition']) for row in config['steps']]}
    case unexpected:
      assert_never(unexpected)
  print(json.dumps(result, ensure_ascii=True, allow_nan=True))


if __name__ == '__main__':
  main()
