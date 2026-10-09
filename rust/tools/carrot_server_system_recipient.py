#!/usr/bin/env python3
from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import sys
import time
from typing import assert_never


def main() -> None:
  root = Path(os.environ['SYSTEM_FIXTURE_ROOT']).resolve()
  assert root.is_relative_to(Path(__file__).resolve().parents[2] / '.omo/evidence/225-system')
  name = Path(sys.argv[0]).name
  args = [name, *sys.argv[1:]]
  with (root / 'commands.jsonl').open('a') as stream:
    fcntl.flock(stream, fcntl.LOCK_EX)
    stream.write(json.dumps({'argv': args, 'pid': os.getpid(), 'at': time.monotonic()}) + '\n')
    stream.flush()
  if name == 'nmcli':
    if 'wifi' in args:
      while (root / 'hold-network').exists():
        time.sleep(0.002)
      sys.stdout.write('yes:Owned\\:wifi:WPA2:70\n')
    else:
      sys.stdout.write('IP4.ADDRESS[1]:192.0.2.10/24\n')
    return
  assert name == 'sudo'
  assert args[1] in {'reboot', 'rm', 'ln', 'date'}
  failure = root / 'fail-command'
  if failure.exists() and failure.read_text().strip() == args[1]:
    raise SystemExit(2)
  match args[1:]:
    case ['reboot'] | ['date', '-s', _]:
      return
    case ['rm', '-f', destination]:
      assert Path(destination) == root / 'localtime'
      Path(destination).unlink(missing_ok=True)
    case ['ln', '-s', source, destination]:
      assert Path(destination) == root / 'localtime'
      assert Path(source).is_relative_to(root / 'zones')
      os.symlink(source, destination)
    case unexpected:
      assert_never(unexpected)


if __name__ == '__main__':
  main()
