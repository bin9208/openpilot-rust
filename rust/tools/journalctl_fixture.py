"""Synthetic OS journalctl seam; journald itself is always the real implementation."""
from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import sys
import time

from journal_fixture_input import Terminated, input_lines


def main():
  assert sys.argv[1:] == ['-f', '-o', 'json'], sys.argv
  trace = Path(os.environ['JOURNAL_FIXTURE_TRACE'])

  def record(event: str) -> None:
    with trace.open('a') as stream:
      stream.write(json.dumps({'event': event, 'pid': os.getpid(), 'ppid': os.getppid(),
                               'monotonic_ns': time.monotonic_ns(), 'argv': sys.argv[1:]}) + '\n')

  with input_lines() as lines:
    try:
      record('started')
      for line in lines:
        command = json.loads(line)
        match command['op']:
          case 'write':
            payload = base64.b64decode(command['base64'])
            while payload:
              payload = payload[os.write(1, payload):]
          case 'close':
            os.close(1)
            record('stdout-closed')
          case 'exit':
            record('exited')
            return command['status']
          case _:
            raise ValueError(command)
    except Terminated as error:
      record(f'signal-{error.signum}')
      return 0
  record('stdin-eof')
  return 0


if __name__ == '__main__':
  raise SystemExit(main())
