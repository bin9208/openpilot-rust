"""Execute original updated.py bodies with owned paths, clock and AGNOS seams."""

import ast
import builtins
from collections import defaultdict
import datetime
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from types import ModuleType, SimpleNamespace

from original_params_binding import load
from openpilot.system.updated.process import run as original_run

ROOT = Path(__file__).resolve().parents[2]


def definitions(path, namespace):
  tree = ast.parse((ROOT / path).read_text())
  tree.body = [node for node in tree.body if not isinstance(node, (ast.Import, ast.ImportFrom)) and not isinstance(node, ast.If)]
  exec(compile(tree, path, 'exec'), namespace)


def run(config, binding):
  root = Path(config['paths']['system_root'])
  os.environ.pop('OPENPILOT_PREFIX', None)
  os.environ['UPDATER_STAGING_ROOT'] = config['paths']['staging']
  os.environ['UPDATER_LOCK_FILE'] = config['paths']['lock']
  module, _ = load(binding, 'ipc://' + str(root / 'source-log.sock'), root / 'source-logs')
  params = module.Params(str(root / 'data/params'))
  current = {'now': config['now'], 'index': 0, 'updater': None}
  commands, snapshots, agnos_calls = [], [], []

  def execute(command, cwd=None):
    commands.append({'argv': command, 'cwd': str(cwd) if cwd is not None else None})
    return original_run(command, cwd)

  def sync_owned():
    descriptor = os.open(root, os.O_RDONLY)
    try:
      os.fsync(descriptor)
    finally:
      os.close(descriptor)

  os_proxy = SimpleNamespace(**{name: getattr(os, name) for name in dir(os) if not name.startswith('__')})
  os_proxy.sync = sync_owned

  def alert(key, show, extra_text=None):
    if show:
      alerts = json.loads((ROOT / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())
      value = dict(alerts[key], extra=extra_text or '')
      params.put(key, value)
    else:
      params.remove(key)

  def open_owned(path, *args, **kwargs):
    if str(path) == '/sys/firmware/devicetree/base/model':
      path = root / 'sys/firmware/devicetree/base/model'
    return builtins.open(path, *args, **kwargs)

  namespace = {
    'open': open_owned,
    'os': os_proxy,
    're': re,
    'datetime': SimpleNamespace(
      datetime=SimpleNamespace(now=lambda tz: datetime.datetime.fromtimestamp(current['now'], tz), fromtimestamp=datetime.datetime.fromtimestamp),
      UTC=datetime.UTC,
      timedelta=datetime.timedelta,
    ),
    'subprocess': subprocess,
    'psutil': SimpleNamespace(Process=lambda: None, LINUX=False),
    'shutil': shutil,
    'signal': signal,
    'fcntl': fcntl,
    'time': time,
    'threading': threading,
    'defaultdict': defaultdict,
    'Path': Path,
    'BASEDIR': config['paths']['base'],
    'Params': lambda: params,
    'system_time_valid': lambda: datetime.datetime(2025, 2, 21) < datetime.datetime.fromtimestamp(current['now']) < datetime.datetime(2035, 1, 1),
    'cloudlog': SimpleNamespace(
      info=lambda *_args, **_kwargs: None, warning=lambda *_args: None, exception=lambda *_args: None, event=lambda *_args, **_kwargs: None
    ),
    'set_offroad_alert': alert,
    'AGNOS': config['agnos'],
    'HARDWARE': SimpleNamespace(get_device_type=lambda: config['device'], get_os_version=lambda: config['os_version']),
    'get_build_metadata': lambda: SimpleNamespace(
      tested_channel=json.loads((Path(config['paths']['base']) / 'build.json').read_text())['channel'] == 'nightly'
    ),
    'run': execute,
  }
  definitions('openpilot/common/markdown.py', namespace)
  definitions('openpilot/system/updated/updated.py', namespace)
  original_updater = namespace['Updater']

  def updater_factory():
    current['updater'] = original_updater()
    return current['updater']

  namespace['Updater'] = updater_factory
  agnos = ModuleType('openpilot.system.hardware.tici.agnos')

  def target_slot():
    agnos_calls.append({'kind': 'slot', 'result': 1})
    return 1

  agnos.get_target_slot_number = target_slot

  def flash(manifest, slot, _logger):
    agnos_calls.append({'kind': 'flash', 'manifest': str(manifest), 'slot': slot})
    assert Path(manifest).is_file() and slot == 1

  agnos.flash_agnos_update = flash
  sys.modules[agnos.__name__] = agnos

  def snapshot(wait):
    updater = current['updater']
    snapshots.append(
      {
        'wait': wait,
        'params': {p.name: p.read_bytes().hex() for p in (root / 'data/params/d').iterdir() if p.is_file()},
        'branches': dict(updater.branches),
        'has_internet': updater.has_internet,
        'consistent': (Path(config['paths']['staging']) / 'finalized/.overlay_consistent').is_file(),
      }
    )

  class StopLoop(BaseException):
    pass

  class Wait:
    def __init__(self):
      self.ready_event = threading.Event()
      self.user_request = 0
      self.apply(config['steps'][0])

    def apply(self, step):
      if step.get('request') is not None:
        self.user_request = {'none': 0, 'check': 1, 'fetch': 2}[step['request']]
      if step.get('now') is not None:
        current['now'] = step['now']
      for key, value in step.get('params', {}).items():
        # Raw files are the fixture input boundary, not a replacement Params implementation.
        (root / 'data/params/d' / key).write_bytes(value.encode())
      if step.get('has_internet') is not None:
        current['updater']._has_internet = step['has_internet']

    def sleep(self, duration):
      snapshot(duration)
      while True:
        current['index'] += 1
        if current['index'] >= len(config['steps']):
          raise StopLoop
        step = config['steps'][current['index']]
        self.apply(step)
        if step.get('report_failure') is None:
          return
        current['updater'].set_params(False, step['report_failure'], 'fixture failure')
        snapshot(None)

  namespace['WaitTimeHelper'] = Wait
  try:
    namespace['main']()
  except StopLoop:
    pass
  return {'snapshots': snapshots, 'commands': commands, 'agnos': agnos_calls}


if __name__ == '__main__':
  config = json.load(sys.stdin)
  print(json.dumps(run(config, Path(sys.argv[1]))))
