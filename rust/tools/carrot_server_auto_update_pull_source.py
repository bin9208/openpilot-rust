from __future__ import annotations

import ast
import asyncio  # Preserve the original process and cancellation behavior.
import fcntl
import json
from pathlib import Path
import sys
import types
from typing import Literal, TypedDict

from carrot_server_git_state_source import original as original_state
from openpilot.common.async_process import run_process
from openpilot.common.repo_update import RepoBusyError, repo_lock

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'openpilot/selfdrive/carrot/server/services/auto_update.py'


class Config(TypedDict):
  repository: str
  lock: str
  state: str
  target: str
  cancel_after_ms: int | None
  cancel_ready: str | None
  alert_failure: bool
  notify_failure: bool


class RecipientFailure(RuntimeError):
  def __init__(self, recipient: Literal['alert', 'notify']) -> None:
    super().__init__(f'owned {recipient} failure')


class ReadinessMissing(TimeoutError):
  def __init__(self) -> None:
    super().__init__('owned reset descendant readiness missing')


async def cancel_after_ready(config: Config, task: asyncio.Task) -> None:
  if config['cancel_ready'] is not None:
    deadline = asyncio.get_running_loop().time() + 2
    ready = Path(config['cancel_ready'])
    while not await asyncio.to_thread(ready.exists):
      if asyncio.get_running_loop().time() >= deadline:
        task.cancel()
        raise ReadinessMissing()
      await asyncio.sleep(.01)
  await asyncio.sleep(config['cancel_after_ms'] / 1000)
  task.cancel()


def original(config: Config, alerts: list, notifications: list, effect_locks: list[bool]):
  state = original_state(Path(config['state']))
  state.time = types.SimpleNamespace(time=lambda: 1700000000.75, time_ns=lambda: 1700000000750000000)
  tree = ast.parse(SOURCE.read_text(), filename=str(SOURCE))
  names = {'_git', '_short_error', '_set_auto_update_alert', '_record_error', '_run_git_pull'}
  tree.body = [node for node in tree.body if (isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in names)
               or (isinstance(node, ast.Assign) and all(isinstance(target, ast.Name) and target.id.isupper() for target in node.targets))]
  module = types.ModuleType('original_auto_update_pull')
  module.__dict__.update({
    'asyncio': asyncio, 'time': state.time, 'run_process': run_process,
    'RepoBusyError': RepoBusyError, 'REPO_DIR': config['repository'],
    'read_auto_update_state': state.read_auto_update_state,
    'write_auto_update_event': state.write_auto_update_event,
    'write_git_pull_time': state.write_git_pull_time,
  })

  def repository_held() -> bool:
    with Path(config['lock']).open('r+b') as descriptor:
      try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
      except BlockingIOError:
        return True
    return False

  def recipient(name: str, show: bool, *, extra_text: str | None) -> None:
    effect_locks.append(repository_held())
    assert name == 'Offroad_CarrotAutoUpdateFailed'
    alerts.append({'show': show, 'detail': extra_text})
    if config['alert_failure']:
      raise RecipientFailure('alert')

  async def notify(old_head: str) -> None:
    await asyncio.sleep(.01)
    effect_locks.append(repository_held())
    notifications.append(old_head)
    if config['notify_failure']:
      raise RecipientFailure('notify')

  recipient_module = types.ModuleType('openpilot.selfdrive.selfdrived.alertmanager')
  recipient_module.set_offroad_alert = recipient
  sys.modules[recipient_module.__name__] = recipient_module
  module._notify_cwp = notify
  exec(compile(tree, str(SOURCE), 'exec'), module.__dict__)
  return module, state


async def execute(config: Config):
  alerts: list = []
  notifications: list = []
  effect_locks: list[bool] = []
  module, state = original(config, alerts, notifications, effect_locks)
  timer = None
  try:
    with repo_lock():
      task = asyncio.create_task(module._run_git_pull(config['target']))
      if config['cancel_after_ms'] is not None:
        timer = asyncio.create_task(cancel_after_ready(config, task))
      try:
        output = {'result': list(await task)}
      except RepoBusyError as error:
        output = {'exception': 'RepoBusyError', 'message': str(error)}
      except asyncio.CancelledError as error:
        output = {'exception': 'CancelledError', 'message': str(error)}
      if timer is not None:
        await timer
  finally:
    if timer is not None:
      timer.cancel()
  return {'output': output, 'state': state.read_git_state(), 'alerts': alerts, 'notifications': notifications, 'effect_locks': effect_locks}


def main() -> None:
  config: Config = json.loads(sys.stdin.read())
  print(json.dumps(asyncio.run(execute(config)), ensure_ascii=True), flush=True)


if __name__ == '__main__':
  main()
