from __future__ import annotations

import json
from pathlib import Path
import re
import signal
import subprocess


def stop_cleanly(process: subprocess.Popen, receipt: Path, timeout: float = 5) -> None:
  signaled = process.poll() is None
  if signaled:
    process.send_signal(signal.SIGTERM)
  timed_out = False
  try:
    status = process.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
    timed_out = True
    process.kill()
    status = process.wait(timeout=5)
  receipt.write_text(json.dumps({'pid': process.pid, 'signaled': signaled, 'timed_out': timed_out, 'returncode': status}) + '\n')
  if timed_out:
    raise AssertionError(f'encoder shutdown timed out: {receipt}')
  if status != 0:
    raise AssertionError(f'encoder shutdown failed with status {status}: {receipt}')


def check_outcome(status: int, output: str, expected_failure: tuple[int, str] | None = None) -> None:
  if re.search(r'(?:Address|Leak|Memory|Thread|UndefinedBehavior)Sanitizer|runtime error:', output):
    raise AssertionError('encoder checker observed a sanitizer failure')
  if expected_failure is None:
    if status != 0:
      raise AssertionError(f'unexpected encoder status {status}')
    return
  expected_status, diagnostic = expected_failure
  if status != expected_status or diagnostic not in output:
    raise AssertionError(f'expected encoder status {expected_status} and {diagnostic!r}, got status {status}: {output}')
