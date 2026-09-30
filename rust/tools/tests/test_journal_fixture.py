# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest==9.0.2"]
# ///
# Run: PYTHONPATH=rust/tools python -m pytest -c /dev/null rust/tools/tests/test_journal_fixture.py
"""Regression for SIGTERM reentering the synthetic journal child's buffered trace."""
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
from typing import TextIO

import journalctl_fixture
import pytest


@dataclass(frozen=True, slots=True)
class SignalAfterWrite:
  stream: TextIO

  def write(self, text: str) -> int:
    count = self.stream.write(text)
    if json.loads(text)['event'] == 'stdout-closed':
      os.kill(os.getpid(), signal.SIGTERM)
    return count


def signal_during_append() -> None:
  """Deliver a real signal after buffering the close record but before flushing it."""
  original_open = Path.open

  @contextmanager
  def open_trace(path: Path, mode: str) -> Iterator[SignalAfterWrite]:
    with original_open(path, mode) as stream:
      yield SignalAfterWrite(stream)

  sys.argv = ['journalctl', '-f', '-o', 'json']
  sys.stdin = io.StringIO('{"op":"close"}\n')
  with pytest.MonkeyPatch.context() as patched:
    patched.setattr(Path, 'open', open_trace)
    raise SystemExit(journalctl_fixture.main())


def test_signal_is_last_when_it_interrupts_buffered_trace_append(tmp_path: Path) -> None:
  # Given the actual child fixture and a signal injected at the buffered-write boundary.
  trace = tmp_path / 'trace.jsonl'
  environment = dict(os.environ, JOURNAL_FIXTURE_TRACE=str(trace))
  # When closing stdout causes termination during the trace append.
  result = subprocess.run([sys.executable, __file__], env=environment, capture_output=True, timeout=5)
  # Then the trace preserves termination as the final event, as the journal oracle requires.
  assert result.returncode == 0, result.stderr.decode()
  events = [json.loads(line) for line in trace.read_text().splitlines()]
  assert events[-1]['event'] == 'signal-15', events


if __name__ == '__main__':
  signal_during_append()
