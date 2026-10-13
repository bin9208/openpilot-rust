#!/usr/bin/env python3
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from typing import assert_never

from carrot_server_auto_update_pull_cases import Mode


def record(kind: str) -> None:
  lock = Path(os.environ['CARROT_REPO_LOCK_PATH'])
  inherited = []
  for entry in Path('/proc/self/fd').iterdir():
    try:
      same = os.path.samestat(os.fstat(int(entry.name)), lock.stat())
    except OSError as error:
      if error.errno != 9:
        raise
      same = False
    if same:
      inherited.append(int(entry.name))
  assert inherited, 'Repository lock was not inherited'
  blocked = False
  with lock.open('r+b') as contender:
    try:
      fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      blocked = True
  assert blocked, 'Repository lock was released during a child command'
  row = {
    'kind': kind, 'argv': sys.argv[1:], 'cwd': os.getcwd(),
    'pid': os.getpid(), 'pgid': os.getpgrp(),
    'starttime': Path('/proc/self/stat').read_text().rsplit(')', 1)[1].split()[19],
    'inherited_lock': bool(inherited), 'contender_blocked': blocked,
  }
  with Path(os.environ['OWNED_PULL_LOG']).open('a') as log:
    log.write(json.dumps(row) + '\n')
    log.flush()
    os.fsync(log.fileno())


def held_group() -> None:
  signal.signal(signal.SIGTERM, signal.SIG_IGN)
  child = os.fork()
  if child == 0:
    record('descendant')
    if os.environ['OWNED_PULL_WRITE_READY'] == '1':
      Path(os.environ['OWNED_PULL_READY']).write_text('owned reset descendant ready\n')
  while True:
    time.sleep(10)


def main() -> None:
  repository = Path(os.environ['OWNED_PULL_REPOSITORY']).resolve()
  assert Path.cwd().resolve() == repository and (repository / '.git').is_dir()
  root = subprocess.check_output(['/usr/bin/git', 'rev-parse', '--show-toplevel'], text=True).strip()
  assert Path(root).resolve() == repository
  record('git')
  args = sys.argv[1:]
  mode = Mode(os.environ['OWNED_PULL_MODE'])
  count_path = Path(os.environ['OWNED_PULL_COUNTER'])
  count = int(count_path.read_text()) if count_path.exists() else 0
  if args == ['rev-parse', 'HEAD']:
    count += 1
    count_path.write_text(str(count))
    if count == 2:
      match mode:
        case Mode.POST_HEAD_FAIL:
          os.write(2, b'owned verification failure\n')
          raise SystemExit(7)
        case Mode.POST_HEAD_MISMATCH:
          print(os.environ['OWNED_PULL_OTHER_HEAD'])
          return
        case (Mode.NORMAL | Mode.RESET_BUSY | Mode.MERGE_BUSY | Mode.RESET_OUTPUT | Mode.RESET_SIGNAL |
              Mode.UPDATED_STATE_FAIL | Mode.CANCEL_GROUP | Mode.CANCEL_NO_READY | Mode.RESET_EXEC_DENIED):
          pass
        case unreachable:
          assert_never(unreachable)
    if count == 1 and mode == Mode.RESET_EXEC_DENIED:
      wrapper = Path(sys.argv[0]).resolve()
      assert wrapper.parent == repository.parent / 'bin'
      wrapper.chmod(0o644)
  if args == ['reset', '--hard']:
    match mode:
      case Mode.RESET_OUTPUT:
        os.write(2, b' \x1cfirst \xff\r\n')
        os.write(1, ' second\t오류\x1f '.encode())
        raise SystemExit(23)
      case Mode.RESET_SIGNAL:
        os.write(1, b'owned reset signal\n')
        os.kill(os.getpid(), signal.SIGTERM)
      case Mode.CANCEL_GROUP | Mode.CANCEL_NO_READY:
        held_group()
      case Mode.MERGE_BUSY:
        run = subprocess.run(['/usr/bin/git', *args], check=False)
        if run.returncode == 0:
          (repository / '.git/index.lock').write_text('owned active lock\n')
        raise SystemExit(run.returncode)
      case (Mode.NORMAL | Mode.RESET_BUSY | Mode.POST_HEAD_FAIL | Mode.POST_HEAD_MISMATCH |
            Mode.UPDATED_STATE_FAIL | Mode.RESET_EXEC_DENIED):
        pass
      case unreachable:
        assert_never(unreachable)
  if args[:2] == ['merge', '--ff-only'] and mode == 'updated-state-fail':
    run = subprocess.run(['/usr/bin/git', *args], check=False)
    if run.returncode == 0:
      (Path(os.environ['OWNED_PULL_STATE']) / 'git.json.tmp').mkdir()
    raise SystemExit(run.returncode)
  os.execv('/usr/bin/git', ['/usr/bin/git', *args])


if __name__ == '__main__':
  main()
