from __future__ import annotations

import ast
import asyncio  # Run the original asyncio lifecycle and cancellation unchanged.
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import types

from carrot_server_auto_update_pull_source import original as pull_original
from carrot_server_git_status_source import original as status_original
from openpilot.common import repo_update
from openpilot.common.async_process import prepare_repo, run_locked_thread
from openpilot.selfdrive.carrot.server.services.git_config import prepare_git_pull

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/carrot/server/services/auto_update.py'


class OwnedParams:
  def __init__(self, root: Path) -> None:
    self.directory = root / 'd'
    self.directory.mkdir(parents=True, exist_ok=True)

  def get(self, name: str) -> bytes | None:
    try:
      return (self.directory / name).read_bytes()
    except FileNotFoundError:
      return None

  def put_bool(self, name: str, value: bool) -> None:
    (self.directory / name).write_bytes(b'1' if value else b'0')

  def put(self, name: str, value: dict) -> None:
    (self.directory / name).write_bytes(json.dumps(value).encode())

  def remove(self, name: str) -> None:
    (self.directory / name).unlink(missing_ok=True)


class ReplaySubMaster:
  def __init__(self, steps: list[dict], now: list[float]) -> None:
    self.steps, self.now, self.index = steps, now, 0
    self.valid, self.alive, self.values = {}, {}, {}

  def update(self, timeout: int) -> None:
    assert timeout == 0
    step = self.steps[self.index]
    self.now[0] = step.get('now', 0.)
    for service, key in [('selfdriveState', 'valid'), ('carState', 'car_valid'), ('deviceState', 'device_valid')]:
      self.valid[service] = self.alive[service] = step.get(key, False)
    self.values = {
      'selfdriveState': types.SimpleNamespace(enabled=step.get('engaged', False)),
      'carState': types.SimpleNamespace(gearShifter=step.get('gear', 'other')),
      'deviceState': types.SimpleNamespace(started=step.get('started', True)),
    }

  def __getitem__(self, name: str):
    return self.values[name]


def original(config: dict):
  effects: list = []
  module, state = pull_original(config | {'target': '', 'alert_failure': False, 'notify_failure': False}, effects, [], [])
  module.__package__ = 'openpilot.selfdrive.carrot.server.services'
  tree = ast.parse(SOURCE.read_text(), filename=str(SOURCE))
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))]
  now = [0.]
  settings = Path(config['state']) / 'web_settings.json'
  module.__dict__.update({'re': re, 'os': os, 'subprocess': subprocess,
    'time': types.SimpleNamespace(monotonic=lambda: now[0], time=lambda: 1700000000.75),
    'read_web_settings': lambda: json.loads(settings.read_text()) if settings.exists() else {},
    'prepare_repo': prepare_repo, 'run_locked_thread': run_locked_thread,
    'prepare_git_pull': prepare_git_pull, 'repo_lock': repo_update.repo_lock, '_last_pull_at': float('-inf')})
  exec(compile(tree, str(SOURCE), 'exec'), module.__dict__)
  status = status_original()
  status.REPO_DIR, repo_update.LOCK_PATH = config['repository'], config['lock']
  status._now = lambda: 1000.
  module.get_git_status, module.clear_git_status_cache = status.get_git_status, status.clear_git_status_cache
  params = OwnedParams(Path(config['params']))
  parameter_module = types.ModuleType('openpilot.common.params')
  parameter_module.Params = lambda: params
  sys.modules[parameter_module.__name__] = parameter_module
  if config['mode'] in {'runtime', 'app', 'wait-reboot'}:
    definitions = json.loads((Path(config['source']) / 'openpilot/selfdrive/selfdrived/alerts_offroad.json').read_text())

    def alert(name: str, show: bool, *, extra_text: str | None) -> None:
      if show:
        params.put(name, definitions[name] | {'extra': extra_text or ''})
      else:
        params.remove(name)

    sys.modules['openpilot.selfdrive.selfdrived.alertmanager'].set_offroad_alert = alert
  return module, state, now, effects, params


def output(state, effects: list, params: OwnedParams, creates: int = 0) -> dict:
  return {'state': state.read_git_state(), 'effects': effects, 'creates': creates,
    'reboot': (params.get('DoReboot') or b'').decode(),
    'alert': (params.get('Offroad_CarrotAutoUpdateFailed') or b'').decode()}


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  module, state, now, effects, params = original(config)
  mode = config['mode']
  creates = 0
  if mode == 'reboot':
    from openpilot.cereal import messaging
    sm = ReplaySubMaster(config['steps'], now)
    messaging.SubMaster = lambda names: sm
    module._auto_reboot_mode = lambda: config['steps'][sm.index].get('mode', config['reboot_mode'])
    original_sleep = asyncio.sleep

    async def advance(seconds: float) -> None:
      assert seconds == .1
      sm.index += 1
      if sm.index >= len(sm.steps):
        raise asyncio.CancelledError
      await original_sleep(0)

    module.asyncio = types.SimpleNamespace(sleep=advance)
    try:
      await module._wait_for_auto_reboot(config['reboot_mode'], config['head'])
    except asyncio.CancelledError:
      pass
  elif mode in {'notify', 'clear'}:
    with repo_update.repo_lock():
      if mode == 'notify':
        await module._notify_cwp(config['head'])
      else:
        module.clear_recovered_git_ref_error()
  elif mode == 'post':
    from openpilot.selfdrive.carrot.cweb_push import post_json
    ok, status, body = await asyncio.to_thread(post_json, config['head'], config['steps'][0], 4.)
    print(json.dumps({'ok': ok, 'status': status, 'body': body}), flush=True)
    return
  else:
    from openpilot.cereal import messaging
    native_submaster = messaging.SubMaster

    def subscribed(names):
      nonlocal creates
      creates += 1
      if creates <= config.get('failures', 0):
        raise RuntimeError('owned unavailable messaging')
      return native_submaster(names)

    messaging.SubMaster = subscribed
    monitor = module.ManagerMonitor()
    if mode == 'manager':
      print('{"ready":true}', flush=True)
      while line := await asyncio.to_thread(sys.stdin.readline):
        command = json.loads(line)
        if command.get('stop'):
          break
        now[0] = command['now']
        print(json.dumps({'sample': monitor.ready(), 'creates': creates}), flush=True)
    else:
      task = asyncio.create_task(module.auto_update_loop()) if mode in {'runtime', 'app'} else asyncio.create_task(
        module._wait_for_auto_reboot(config['reboot_mode'], config['head']))
      print('{"ready":true}', flush=True)
      try:
        while line := await asyncio.to_thread(sys.stdin.readline):
          command = json.loads(line)
          if command.get('stop'):
            break
          now[0] = command.get('now', now[0])
          print(json.dumps(output(state, effects, params, creates)), flush=True)
      finally:
        task.cancel()
        try:
          await task
        except asyncio.CancelledError:
          pass
  print(json.dumps(output(state, effects, params, creates)), flush=True)


if __name__ == '__main__':
  asyncio.run(main())
