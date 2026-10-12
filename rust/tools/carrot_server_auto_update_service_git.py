#!/usr/bin/env python3
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main() -> None:
  repository = Path(os.environ['OWNED_AUTO_REPOSITORY'])
  assert Path.cwd().resolve() == repository.resolve() and (repository / '.git').is_dir()
  top = subprocess.check_output(['/usr/bin/git', 'rev-parse', '--show-toplevel'], text=True).strip()
  assert Path(top).resolve() == repository.resolve()
  lock = Path(os.environ['CARROT_REPO_LOCK_PATH'])
  inherited = any(os.path.realpath(path) == str(lock) for path in Path('/proc/self/fd').iterdir())
  held = False
  with lock.open('a+b') as file:
    try:
      fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      held = True
  arguments = sys.argv[1:]
  row = {'pid': os.getpid(), 'start': Path('/proc/self/stat').read_text().split()[21], 'argv': arguments, 'held': held, 'inherited': inherited}
  descriptor = os.open(os.environ['OWNED_AUTO_LOG'], os.O_CREAT | os.O_APPEND | os.O_WRONLY, 0o600)
  try:
    os.write(descriptor, (json.dumps(row) + '\n').encode())
  finally:
    os.close(descriptor)
  mode = os.environ.get('OWNED_AUTO_MODE', '')
  holding = (
    (mode in {'hold-fetch', 'stop-fetch'} and arguments[0] == 'fetch')
    or (mode in {'hold-config', 'app-config'} and arguments[0] == 'ls-remote')
    or (mode == 'stop-notify' and arguments[:2] == ['log', '--pretty=%h|%s'])
  )
  if holding:
    Path(os.environ['OWNED_AUTO_READY']).write_text(json.dumps(row))
    while not Path(os.environ['OWNED_AUTO_RELEASE']).exists():
      time.sleep(.005)
  os.execv('/usr/bin/git', ['git', *arguments])


if __name__ == '__main__':
  main()
