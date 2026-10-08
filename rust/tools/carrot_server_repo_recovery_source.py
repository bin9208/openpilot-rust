from __future__ import annotations

import asyncio  # Execute the unchanged source cancellation contract.
import fcntl
import json
import os
from pathlib import Path
import shutil
import sys
import subprocess
import time
import types
from typing import Literal, NotRequired, TypedDict, assert_never

from openpilot.common import repo_update
from openpilot.common.async_process import prepare_repo


Action = Literal['none', 'remove', 'replace', 'size', 'mtime', 'git_start', 'proc_remove']


class Config(TypedDict):
  repo: str
  launcher: str
  lock: str
  proc_root: str
  now: float
  action: Action
  cancel: bool
  python: NotRequired[str]
  source: NotRequired[str]


class Output(TypedDict, total=False):
  result: bool
  exception: str
  message: str


class Result(TypedDict):
  output: Output
  locks: list[bool]


def held(path: Path) -> bool:
  with path.open('r+b') as file:
    try:
      fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      return True
  return False


def mutate(config: Config) -> None:
  path = Path(config['repo']) / '.git/index.lock'
  match config['action']:
    case 'none':
      pass
    case 'remove':
      path.unlink()
    case 'replace':
      replacement = path.with_name('replacement')
      replacement.write_bytes(path.read_bytes())
      before = path.stat()
      os.utime(replacement, ns=(before.st_atime_ns, before.st_mtime_ns))
      replacement.replace(path)
    case 'size':
      before = path.stat()
      with path.open('ab') as file:
        file.write(b'changed')
      os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns))
    case 'mtime':
      before = path.stat()
      os.utime(path, ns=(before.st_atime_ns, before.st_mtime_ns + 1))
    case 'git_start':
      process = Path(config['proc_root']) / '99'
      process.mkdir()
      (process / 'comm').write_text('git-fetch\n')
    case 'proc_remove':
      shutil.rmtree(config['proc_root'])
    case unreachable:
      assert_never(unreachable)


async def execute(config: Config) -> Result:
  proc_root = Path(config['proc_root'])
  repo_update.Path = lambda value: proc_root if value == '/proc' else Path(value)
  readiness = Path(config['repo']).parent / 'pause-entered'
  release = readiness.with_name('pause-release')
  locks: list[bool] = []

  def pause(seconds: float) -> None:
    assert seconds == .1
    locks.append(held(Path(config['lock'])))
    readiness.write_text('owned inspection recheck entered\n')
    if config['cancel']:
      deadline = time.monotonic() + 2
      while not release.exists():
        if time.monotonic() >= deadline:
          raise TimeoutError('owned recheck release missing')
        time.sleep(.005)
    time.sleep(seconds)
    mutate(config)
    locks.append(held(Path(config['lock'])))

  repo_update.time = types.SimpleNamespace(time=lambda: config['now'], sleep=pause)

  async def cancel(task: asyncio.Task) -> None:
    deadline = asyncio.get_running_loop().time() + 2
    while not await asyncio.to_thread(readiness.exists):
      if asyncio.get_running_loop().time() >= deadline:
        raise TimeoutError('owned recheck readiness missing')
      await asyncio.sleep(.005)
    task.cancel()
    await asyncio.sleep(.02)
    locks.append(held(Path(config['lock'])))
    release.write_text('release owned inspection after cancellation\n')

  with repo_update.repo_lock():
    task = asyncio.create_task(prepare_repo(config['repo']))
    timer = asyncio.create_task(cancel(task)) if config['cancel'] else None
    output: Output
    try:
      output = {'result': await task}
    except asyncio.CancelledError as error:
      output = {'exception': 'CancelledError', 'message': str(error)}
    except (repo_update.RepoBusyError, OSError, RuntimeError, UnicodeDecodeError, subprocess.TimeoutExpired) as error:
      output = {'exception': type(error).__name__, 'message': str(error)}
    if timer is not None:
      await timer
  return Result(output=output, locks=locks)


def main() -> None:
  config: Config = json.load(sys.stdin)
  if sys.argv[1:2] == ['--mutate']:
    mutate(config)
    return
  print(json.dumps(asyncio.run(execute(config)), ensure_ascii=True), flush=True)


if __name__ == '__main__':
  main()
