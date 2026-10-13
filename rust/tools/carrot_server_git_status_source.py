from __future__ import annotations

import asyncio
import importlib.util
import json
from pathlib import Path
import sys


def original():
  path = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server/services/git_status.py'
  spec = importlib.util.spec_from_file_location('original_git_status', path)
  if spec is None or spec.loader is None:
    raise RuntimeError('original Git status import unavailable')
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  module = original()
  from openpilot.common import repo_update
  repo_update.LOCK_PATH = config['lock']
  module.REPO_DIR = config['repo']
  now = 1000.0
  module._now = lambda: now
  poll = None
  while line := await asyncio.to_thread(sys.stdin.readline):
    step = json.loads(line)
    match step['operation']:
      case 'get':
        now = step['now']
        output = await module.get_git_status(force=step['force'])
      case 'group':
        now = step['now']
        output = await asyncio.gather(*(module.get_git_status(force=step['force']) for _ in range(step['count'])))
      case 'clear':
        module.clear_git_status_cache()
        output = {'cleared': True}
      case 'cancel':
        task = asyncio.create_task(module.get_git_status(force=True))
        await asyncio.to_thread(lambda: Path(step['notice']).open('rb').read(1))
        task.cancel()
        try:
          await task
        except asyncio.CancelledError:
          pass
        else:
          raise RuntimeError('owned source request did not cancel')
        output = {'cancelled': True}
      case 'cancel_immediate':
        task = asyncio.create_task(module.get_git_status(force=True))
        await asyncio.sleep(0)
        task.cancel()
        try:
          await task
        except asyncio.CancelledError:
          pass
        else:
          raise RuntimeError('initial source request did not cancel')
        output = {'cancelled': True}
      case 'queued_poll_cancel':
        api = asyncio.create_task(module.get_git_status(force=True))
        await asyncio.to_thread(lambda: Path(step['notice']).open('rb').read(1))
        periodic = asyncio.create_task(module.git_status_loop(interval=0.06, initial_delay=0))
        await asyncio.sleep(0)
        if not module._lock._waiters or not module._lock.locked():
          raise RuntimeError('owned source poll did not queue behind API')
        periodic.cancel()
        try:
          await asyncio.wait_for(periodic, 0.5)
        except asyncio.CancelledError:
          stopped = True
        except asyncio.TimeoutError:
          stopped = False
        running = not api.done()
        api.cancel()
        try:
          await api
        except asyncio.CancelledError:
          pass
        output = {'stopped_while_api_running': stopped and running}
      case 'poll':
        if poll is not None:
          raise RuntimeError('owned poll already running')
        poll = asyncio.create_task(module.git_status_loop(interval=step['interval_ms'] / 1000, initial_delay=step['initial_ms'] / 1000))
        output = {'polling': True}
      case 'stop_poll':
        if poll is not None:
          poll.cancel()
          try:
            await poll
          except asyncio.CancelledError:
            pass
          poll = None
        output = {'stopped': True}
      case operation:
        raise ValueError(f'unknown owned operation: {operation}')
    print(json.dumps(output), flush=True)
  if poll is not None:
    poll.cancel()
    try:
      await poll
    except asyncio.CancelledError:
      pass


if __name__ == '__main__':
  asyncio.run(main())
