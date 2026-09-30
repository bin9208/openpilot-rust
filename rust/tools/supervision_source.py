#!/usr/bin/env python3
"""Original manager process class bodies with isolated paths and native daemon fixture targets."""
import ast
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from abc import ABC, abstractmethod
from collections.abc import Callable, ValuesView
from multiprocessing import Process
from types import SimpleNamespace

from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def original(config):
  binding, swaglog = load(Path(config['binding']), config['endpoint'], Path(config['params_root']) / 'logs')
  from openpilot.cereal import car, log

  def daemon_popen(arguments, **kwargs):
    assert arguments[:2] == ['python', '-m'] and len(arguments) == 3
    assert kwargs['preexec_fn'] is os.setpgrp
    assert all(Path(kwargs[key].name) == Path('/dev/null') for key in ['stdin', 'stdout', 'stderr'])
    targets = [p['argv'] for p in config['processes'] if p['kind'] == 'persistent' and p['identity'] == arguments[2]]
    assert len(targets) == 1
    return subprocess.Popen(targets[0], **kwargs)

  scope = {'os': os, 'signal': signal, 'time': time, 'Process': Process,
           'subprocess': SimpleNamespace(Popen=daemon_popen), 'cloudlog': swaglog.cloudlog,
           'BASEDIR': config['basedir'], 'Params': lambda: binding.Params(config['params_root']),
           'car': car, 'log': log, 'ABC': ABC, 'abstractmethod': abstractmethod,
           'Callable': Callable, 'ValuesView': ValuesView}
  path = ROOT / 'openpilot/system/manager/process.py'
  names = {'nativelauncher', 'join_process', 'ManagerProcess', 'NativeProcess', 'DaemonProcess', 'ensure_running'}
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
  exec(compile(tree, str(path), 'exec'), scope)
  processes = {}
  for spec in config['processes']:
    match spec['kind']:
      case 'native':
        process = scope['NativeProcess'](spec['name'], spec['cwd'], spec['argv'], lambda *_: True,
                                          enabled=spec.get('enabled', True), sigkill=spec.get('sigkill', False))
      case 'persistent':
        process = scope['DaemonProcess'](spec['name'], spec['identity'], spec['param'], enabled=spec.get('enabled', True))
      case other:
        raise ValueError(other)
    process.restart_if_crash = spec.get('restart_if_crash', False)
    processes[spec['name']] = process
  return scope, processes, binding.UnknownKeyName


def snapshots(processes):
  values = []
  for process in processes.values():
    state = process.get_process_state_msg()
    values.append({'state': state.to_dict(), 'wire': list(state.to_bytes()),
                   'has_process': process.proc is not None, 'shutting_down': process.shutting_down})
  return values


def release(race):
  path = Path(race['release'])
  temporary = path.with_suffix('.tmp')
  temporary.write_text('7')
  temporary.replace(path)
  deadline = time.monotonic() + 3
  while True:
    state = Path(f"/proc/{race['pid']}/stat").read_text().rsplit(') ', 1)[1][0]
    if state == 'Z':
      return
    assert time.monotonic() < deadline, 'fixture did not exit during predicate'
    time.sleep(0.001)


def action(scope, processes, request):
  process = processes.get(request.get('name'))
  match request['op']:
    case 'start':
      return process.start()
    case 'restart':
      return process.restart()
    case 'prepare':
      return process.prepare()
    case 'state':
      return process.get_process_state_msg().to_dict()
    case 'stop':
      return process.stop(retry=request.get('retry', True), block=request.get('block', True), sig=request.get('signal'))
    case 'signal':
      return process.signal(request['signal'])
    case 'ensure':
      predicates = []

      def predicate(name):
        def check(*_):
          predicates.append(name)
          race = request.get('race')
          if race is not None and race['name'] == name:
            release(race)
          return name in request['allowed']
        return check

      for process in processes.values():
        process.should_run = predicate(process.name)
      running = scope['ensure_running'](processes.values(), False, not_run=request.get('not_run', []))
      return {'running': [p.name for p in running], 'predicates': predicates}
    case 'exit':
      return None
    case other:
      raise ValueError(other)


def main():
  config = json.loads(sys.stdin.readline())
  scope, processes, unknown_key = original(config)
  print(json.dumps({'ready': True}), flush=True)
  try:
    for line in sys.stdin:
      request = json.loads(line)
      start = time.monotonic()
      try:
        result = action(scope, processes, request)
        error = None
      except (OSError, ValueError, TypeError, OverflowError, unknown_key) as failure:
        result = None
        error = {'kind': type(failure).__name__, 'message': str(failure)}
      elapsed = time.monotonic() - start
      print(json.dumps({'result': result, 'error': error, 'elapsed': elapsed, 'snapshots': snapshots(processes)}), flush=True)
      if request['op'] == 'exit':
        break
  finally:
    for process in processes.values():
      process.stop(sig=signal.SIGKILL)


if __name__ == '__main__':
  main()
