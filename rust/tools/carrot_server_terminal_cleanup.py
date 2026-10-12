# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import os
from pathlib import Path
import signal
import time


def identity(pid: int) -> tuple[int, str] | None:
  try:
    stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return int(stat[19]), stat[0]
  except (FileNotFoundError, ProcessLookupError):
    return None


def close(pid: int, starttime: int) -> dict[str, int | bool]:
  for requested in [signal.SIGTERM, signal.SIGKILL]:
    current = identity(pid)
    if current is None or current[0] != starttime or current[1] == 'Z':
      return {'pid': pid, 'starttime': starttime, 'exited': True}
    try:
      os.kill(pid, requested)
    except ProcessLookupError:
      return {'pid': pid, 'starttime': starttime, 'exited': True}
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
      current = identity(pid)
      if current is None or current[0] != starttime or current[1] == 'Z':
        return {'pid': pid, 'starttime': starttime, 'exited': True}
      time.sleep(0.02)
  raise TimeoutError(f'owned process {pid}/{starttime} did not exit')
