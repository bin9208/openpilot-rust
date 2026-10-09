from __future__ import annotations

import ast
import asyncio  # Execute unchanged original asyncio cancellation and shield behavior.
import fcntl
import json
from pathlib import Path
import subprocess
import sys
import types

from carrot_server_auto_update_attempt_cases import Config, Result
from carrot_server_auto_update_pull_source import original as pull_original
from carrot_server_git_status_source import original as status_original
from openpilot.common import repo_update
from openpilot.common.async_process import prepare_repo, run_locked_thread
from openpilot.selfdrive.carrot.server.services.git_config import prepare_git_pull

SOURCE = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/services/auto_update.py'

def held(path: str) -> bool:
  with Path(path).open('r+b') as file:
    try:
      fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      return True
  return False


async def execute(config: Config) -> Result:
  effects = []
  module, state = pull_original({
    'repository': config['repository'], 'lock': config['lock'], 'state': config['state'], 'target': '',
    'cancel_after_ms': None, 'cancel_ready': None, 'alert_failure': False, 'notify_failure': False,
  }, [], [], [])

  def recipient(name: str, show: bool, *, extra_text: str | None) -> None:
    assert name == 'Offroad_CarrotAutoUpdateFailed'
    effects.append({'show': show, 'detail': extra_text, 'lock_held': held(config['lock'])})

  async def notify(head: str) -> None:
    await asyncio.sleep(0)
    effects.append({'notify': head, 'lock_held': held(config['lock'])})

  sys.modules['openpilot.selfdrive.selfdrived.alertmanager'].set_offroad_alert = recipient
  module._notify_cwp = notify
  status = status_original()
  status.REPO_DIR = config['repository']
  status._now = lambda: 1000.
  repo_update.LOCK_PATH = config['lock']
  proc_root = Path(config['proc_root'])
  repo_update.Path = lambda value: proc_root if value == '/proc' else Path(value)
  now = [0.]
  module.time = types.SimpleNamespace(monotonic=lambda: now[0], time=lambda: 1700000000.75, time_ns=lambda: 1700000000750000000)
  module.__dict__.update({
    'get_git_status': status.get_git_status, 'clear_git_status_cache': status.clear_git_status_cache,
    'prepare_repo': prepare_repo, 'run_locked_thread': run_locked_thread,
    'prepare_git_pull': prepare_git_pull, 'repo_lock': repo_update.repo_lock,
    '_last_pull_at': float('-inf'),
  })
  tree = ast.parse(await asyncio.to_thread(SOURCE.read_text), filename=str(SOURCE))
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
               and node.name in {'_attempt_update', '_verified_update_target'}]
  exec(compile(tree, str(SOURCE), 'exec'), module.__dict__)
  steps = []
  for step in config['steps']:
    await asyncio.to_thread(Path(config['phase']).write_text, 'fixture')
    if step['restore']:
      await asyncio.to_thread(subprocess.run, ['/usr/bin/git', 'reset', '--hard', config['base']],
                              cwd=config['repository'], check=True, capture_output=True)
    if step['warm']:
      await status.get_git_status(force=True)
    await asyncio.to_thread(Path(config['phase']).write_text, 'attempt')
    now[0] = step['now']
    ready = list(step['ready'])
    calls = [0]

    def sample(calls: list[int] = calls, ready: list[bool] = ready) -> bool:
      calls[0] += 1
      return ready.pop(0) if ready else True

    async def cancel(task: asyncio.Task) -> None:
      deadline = asyncio.get_running_loop().time() + 2
      notice = Path(config['cancel_ready'])
      while not await asyncio.to_thread(notice.exists):
        if asyncio.get_running_loop().time() > deadline:
          task.cancel()
          raise TimeoutError('owned configuration readiness missing')
        await asyncio.sleep(.005)
      task.cancel()
      await asyncio.sleep(0)
      effects.append({'cancel_lock_held': held(config['lock'])})
      await asyncio.to_thread(Path(config['cancel_release']).write_text, 'release owned configuration child')

    contender = await asyncio.to_thread(Path(config['lock']).open, 'a+b') if config['busy'] else None
    try:
      if contender is not None:
        fcntl.flock(contender, fcntl.LOCK_EX)
      if config['index_lock']:
        (Path(config['repository']) / '.git/index.lock').write_text('owned current index lock\n')
      task = asyncio.create_task(module._attempt_update(types.SimpleNamespace(ready=sample)))
      timer = asyncio.create_task(cancel(task)) if config['cancel'] else None
      try:
        output = {'result': list(await task)}
      except asyncio.CancelledError as error:
        output = {'exception': 'CancelledError', 'message': str(error)}
      if timer is not None:
        await timer
    finally:
      if contender is not None:
        contender.close()
    await asyncio.to_thread(Path(config['phase']).write_text, 'probe')
    cache = await status.get_git_status()
    steps.append({'output': output, 'ready_calls': calls[0], 'state': state.read_git_state(), 'cache_head': cache.get('head', '')})
  return {'steps': steps, 'effects': effects}


def main() -> None:
  config: Config = json.load(sys.stdin)
  print(json.dumps(asyncio.run(execute(config)), ensure_ascii=True), flush=True)


if __name__ == '__main__':
  main()
