from __future__ import annotations

import fcntl
import json
import os
from enum import StrEnum
from pathlib import Path
import sys
import time
from typing import assert_never


class Mode(StrEnum):
  NORMAL = 'normal'
  STDERR = 'stderr'
  EMPTY_ERROR = 'empty_error'
  STDOUT_UTF8 = 'stdout_utf8'
  STDERR_UTF8 = 'stderr_utf8'
  NEWLINE = 'newline'
  TIMEOUT = 'timeout'


def main() -> None:
  repo = Path(os.environ['OWNED_RECOVERY_REPO'])
  assert Path.cwd() == repo and (repo / '.git').is_dir()
  assert sys.argv[1:] == ['rev-parse', '--path-format=absolute', '--git-path', 'index.lock']
  lock = Path(os.environ['CARROT_REPO_LOCK_PATH'])
  inherited = False
  for entry in Path('/proc/self/fd').iterdir():
    try:
      inherited |= os.path.samestat(os.fstat(int(entry.name)), lock.stat())
    except OSError as error:
      assert error.errno == 9
  with lock.open('r+b') as contender:
    try:
      fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      blocked = True
    else:
      blocked = False
  assert inherited and blocked
  row = {'argv': sys.argv[1:], 'inherited': inherited, 'blocked': blocked, 'pid': os.getpid()}
  Path(os.environ['OWNED_RECOVERY_TRACE']).write_text(json.dumps(row))
  mode = Mode(os.environ['OWNED_RECOVERY_GIT'])
  match mode:
    case Mode.NORMAL:
      os.execv('/usr/bin/git', ['/usr/bin/git', *sys.argv[1:]])
    case Mode.STDERR:
      os.write(1, b'ignored stdout\n')
      os.write(2, b' \x1cowned lookup error\r\n\x1f ')
      raise SystemExit(7)
    case Mode.EMPTY_ERROR:
      raise SystemExit(7)
    case Mode.STDOUT_UTF8:
      os.write(1, b'\xff')
    case Mode.STDERR_UTF8:
      os.write(1, str(repo / '.git/index.lock').encode())
      os.write(2, b'\xff')
    case Mode.NEWLINE:
      os.write(1, b' \x1c' + str(repo / '.git/index.lock').encode() + b'\r\n\x1f ')
      os.write(2, b'ignored valid stderr\r\n')
    case Mode.TIMEOUT:
      time.sleep(30)
    case unreachable:
      assert_never(unreachable)


if __name__ == '__main__':
  main()
