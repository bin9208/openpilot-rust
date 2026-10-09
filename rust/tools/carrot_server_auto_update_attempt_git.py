#!/usr/bin/env python3
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import time
from typing import assert_never

from carrot_server_auto_update_attempt_cases import Mode
from carrot_server_auto_update_pull_git import record


def main() -> None:
  repository = Path(os.environ['OWNED_PULL_REPOSITORY']).resolve()
  assert Path.cwd().resolve() == repository and (repository / '.git').is_dir()
  root = subprocess.check_output(['/usr/bin/git', 'rev-parse', '--show-toplevel'], text=True).strip()
  assert Path(root).resolve() == repository
  record('git')
  phase = Path(os.environ['OWNED_ATTEMPT_PHASE']).read_text()
  with Path(os.environ['OWNED_ATTEMPT_PHASE_LOG']).open('a') as log:
    log.write(str(os.getpid()) + ' ' + phase + '\n')
  args = sys.argv[1:]
  mode = Mode(os.environ['OWNED_ATTEMPT_MODE'])
  if mode == Mode.FETCH_ERROR and args[0] == 'fetch':
    print('owned fetch unavailable', file=sys.stderr)
    raise SystemExit(9)
  if phase == 'attempt':
    match mode:
      case Mode.BRANCH_CHANGE | Mode.HEAD_CHANGE | Mode.TARGET_MOVE:
        if args == ['rev-parse', '--path-format=absolute', '--git-path', 'index.lock']:
          match mode:
            case Mode.BRANCH_CHANGE:
              command = ['symbolic-ref', 'HEAD', 'refs/heads/owned-other']
              cwd = repository
            case Mode.HEAD_CHANGE:
              command = ['update-ref', 'refs/heads/owned-update', os.environ['OWNED_PULL_OTHER_HEAD']]
              cwd = repository
            case Mode.TARGET_MOVE:
              command = ['update-ref', 'refs/heads/owned-update', os.environ['OWNED_PULL_OTHER_HEAD']]
              cwd = Path(os.environ['OWNED_ATTEMPT_REMOTE'])
            case unreachable:
              assert_never(unreachable)
          subprocess.run(['/usr/bin/git', *command], cwd=cwd, check=True)
      case Mode.BRANCH_ERROR:
        if args == ['branch', '--show-current']:
          print('owned branch read failed', file=sys.stderr)
          raise SystemExit(7)
      case Mode.HEAD_ERROR:
        if args == ['rev-parse', 'HEAD']:
          print('owned head read failed', file=sys.stderr)
          raise SystemExit(8)
      case Mode.CONFIG_ERROR:
        if args == ['ls-remote', '--heads', 'origin']:
          print('owned configuration failure', file=sys.stderr)
          raise SystemExit(44)
      case Mode.RESET_BUSY:
        if args == ['reset', '--hard']:
          print('fatal: Unable to create index.lock: File exists', file=sys.stderr)
          raise SystemExit(128)
      case Mode.CONFIG_CANCEL:
        if args == ['rev-parse', '--verify', '@{upstream}^{commit}']:
          Path(os.environ['OWNED_ATTEMPT_CANCEL_READY']).write_text('owned configuration child ready\n')
          deadline = time.monotonic() + 2
          while not Path(os.environ['OWNED_ATTEMPT_CANCEL_RELEASE']).exists():
            if time.monotonic() >= deadline:
              raise TimeoutError('owned configuration release missing')
            time.sleep(.005)
      case Mode.NORMAL | Mode.FETCH_ERROR:
        pass
      case unreachable:
        assert_never(unreachable)
  os.execv('/usr/bin/git', ['/usr/bin/git', *args])


if __name__ == '__main__':
  main()
