#!/usr/bin/env python3
"""Unchanged beep/Ratekeeper bodies with only external clock/hardware/path fixtures."""
import ast
import ctypes
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import threading
import time
from types import SimpleNamespace

from original_params_binding import load

ROOT = Path(__file__).resolve().parents[2]


def definition(path, name, scope):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef)) and node.name == name]
  exec(compile(tree, str(path), 'exec'), scope)
  return scope[name]


def main():
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  # Linux core_pattern pipes ignore RLIMIT_CORE; keep expected SIGABRT probes out of host crash collectors.
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  if libc.prctl(4, 0, 0, 0, 0) != 0:  # PR_SET_DUMPABLE
    raise OSError(ctypes.get_errno(), 'disable fixture core dumping')
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  module, _ = load(Path(config['binding']), 'ipc://' + str(root / 'swaglog'), root / 'logs')
  params = module.Params(str(root))
  if config['mode'] == 'integer':
    print(json.dumps({'value': params.get_int('SoundVolumeAdjust')}), flush=True)
    return
  from openpilot.cereal import car, messaging
  state = {'commands': 0, 'sleeps': 0}
  times = iter(config.get('times', []))
  lock = threading.Lock()
  trace = Path(config['trace']).open('w', buffering=1)

  def record(value):
    with lock:
      trace.write(json.dumps(value) + '\n')

  def command(command, **kwargs):
    index = state['commands']
    state['commands'] += 1
    failure = index in config.get('command_fail', [])
    record({'kind': 'command', 'command': command, 'failed': failure, 'status': config.get('status', 0)})
    assert kwargs == {'shell': True, 'stderr': subprocess.DEVNULL, 'stdout': subprocess.DEVNULL, 'encoding': 'utf8'}
    if failure:
      raise FileNotFoundError('injected command spawn failure')
    return subprocess.CompletedProcess(command, config.get('status', 0))

  def sleep(seconds):
    index = state['sleeps']
    state['sleeps'] += 1
    record({'kind': 'sleep', 'seconds': seconds})
    if str(index) in config.get('mutations', {}):
      (root / os.environ['OPENPILOT_PREFIX'] / 'SoundVolumeAdjust').write_bytes(bytes(config['mutations'][str(index)]))
    if index in config.get('sleep_fail', []):
      raise OSError('injected clock sleep failure')

  scope = {'time': time if config['mode'] == 'live' else SimpleNamespace(sleep=sleep, monotonic=lambda: next(times)),
           'subprocess': subprocess if config['mode'] == 'live' else SimpleNamespace(run=command, DEVNULL=subprocess.DEVNULL),
           'threading': threading, 'Params': lambda: params, 'AudibleAlert': car.CarControl.HUDControl.AudibleAlert,
           'messaging': messaging, 'getproctitle': lambda: 'beep'}
  scope['MovingAverage'] = definition(ROOT / 'openpilot/common/utils.py', 'MovingAverage', scope)
  scope['Ratekeeper'] = definition(ROOT / 'openpilot/common/realtime.py', 'Ratekeeper', scope)
  if config['mode'] == 'rate':
    ratekeeper = scope['Ratekeeper'](20)
    for _ in range(config['count']):
      ratekeeper.keep_time()
      record({'kind': 'rate', 'frame': ratekeeper.frame, 'remaining': ratekeeper.remaining})
    return
  beepd = definition(ROOT / 'openpilot/selfdrive/controls/beep.py', 'Beepd', scope)()
  if config['mode'] == 'live':
    beepd.beepd_thread()
    return
  for action in config['actions']:
    if action['kind'] == 'alert':
      beepd.update_alert(action['value'])
      for worker in threading.enumerate():
        if worker is not threading.current_thread():
          worker.join(timeout=3)
          assert not worker.is_alive()
    elif action['kind'] == 'volume':
      path = root / os.environ['OPENPILOT_PREFIX'] / 'SoundVolumeAdjust'
      if action['bytes'] is None:
        path.unlink(missing_ok=True)
      else:
        path.write_bytes(bytes(action['bytes']))
    else:
      raise ValueError(action)
  trace.close()


if __name__ == '__main__':
  main()
