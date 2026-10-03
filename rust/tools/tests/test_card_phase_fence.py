from __future__ import annotations

from pathlib import Path
import signal
import subprocess
import sys
import time

import pytest
from card_runtime_source import fixture_fence_due
from card_qa.runtime_phase_fence import wait_stopped


def test_explicit_frame_fence_rejects_invalid_and_missed_targets(tmp_path: Path) -> None:
  path = tmp_path / 'fence'
  assert not fixture_fence_due(path, 4, 10)
  path.write_text('5\n')
  assert not fixture_fence_due(path, 4, 10)
  assert fixture_fence_due(path, 5, 10)
  with pytest.raises(ValueError):
    fixture_fence_due(path, 6, 10)
  for invalid in ('-1', '10', 'invalid', '', '5_0', '５'):
    path.write_text(invalid)
    with pytest.raises(ValueError):
      fixture_fence_due(path, 4, 10)
  path.unlink()
  assert not fixture_fence_due(path, 6, 10)


def test_observer_collects_after_real_child_self_stop_before_resuming() -> None:
  code = 'import os,signal; print("complete",flush=True); os.kill(os.getpid(),signal.SIGSTOP); print("resumed",flush=True)'
  with subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE, text=True) as process:
    try:
      wait_stopped(process, lambda: None)
      time.sleep(.05)
      assert process.poll() is None
      assert process.stdout.readline() == 'complete\n'
      process.send_signal(signal.SIGCONT)
      assert process.stdout.readline() == 'resumed\n'
      assert process.wait(timeout=3) == 0
    finally:
      if process.poll() is None:
        process.send_signal(signal.SIGCONT)
        process.kill()
        process.wait(timeout=3)
