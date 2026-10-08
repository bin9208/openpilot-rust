from __future__ import annotations

import json
import os
from pathlib import Path
import select
import shutil
import struct
import subprocess
import time
from typing import TypedDict


class Report(TypedDict):
  completed_waits_before_input: int
  input_queued_before_release: bool
  later_wait_returns: str


def build_library(directory: Path, output: Path) -> Path:
  library = directory / 'startup-phase.so'
  if not library.exists():
    source = Path(__file__).with_name('paramsd_startup_phase.c')
    command = ['clang', '-std=c11', '-O2', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
               str(source), '-ldl', '-o', str(library)]
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    (output / 'phase-build.json').write_text(json.dumps({'argv': command, 'exit': result.returncode,
      'stdout': result.stdout, 'stderr': result.stderr}, indent=2))
  shutil.copy2(library, output / 'startup-phase.so')
  return library


class StartupGate:
  def __init__(self, position: Path, queue: Path, output: Path) -> None:
    self.position, self.queue, self.output = position, queue, output
    self.ready_read, self.ready_write = os.pipe()
    self.release_read, self.release_write = os.pipe()
    self.descriptors = [self.ready_read, self.ready_write, self.release_read, self.release_write]
    self.report = Report(completed_waits_before_input=1, input_queued_before_release=False,
                         later_wait_returns='diagnostic only; not an inferred empty-update count')

  @property
  def inherited(self) -> tuple[int, int]:
    return self.ready_write, self.release_read

  def environment(self, library: Path) -> dict[str, str]:
    return {'LD_PRELOAD': str(library), 'PARAMSD_PHASE_TRACE': str(self.output / 'poll.jsonl'),
      'PARAMSD_PHASE_POSITION': str(self.position), 'PARAMSD_PHASE_READY_FD': str(self.ready_write),
      'PARAMSD_PHASE_RELEASE_FD': str(self.release_read)}

  def spawned(self) -> None:
    for descriptor in (self.ready_write, self.release_read):
      os.close(descriptor)
      self.descriptors.remove(descriptor)

  def wait(self) -> None:
    assert select.select([self.ready_read], [], [], 10)[0], 'startup second-wait gate missing'
    assert os.read(self.ready_read, 1) == b'R', 'startup gate ended before readiness'
    rows = [json.loads(line) for line in (self.output / 'poll.jsonl').read_text().splitlines()]
    assert any(row['event'] == 'gate' and row['completed'] == 1 for row in rows), rows
    assert [row['completed'] for row in rows if row['event'] == 'return' and row['result'] == 0] == [1], rows

  def pointer(self) -> int:
    with self.queue.open('rb') as stream:
      return struct.unpack('<2Q', stream.read(16))[1]

  def release(self, previous: int) -> None:
    deadline = time.monotonic() + 2
    while self.pointer() == previous:
      assert time.monotonic() < deadline, 'first livePose was not actually queued'
      time.sleep(.001)
    self.report['input_queued_before_release'] = True
    (self.output / 'startup-poll-gate.json').write_text(json.dumps(self.report, indent=2))
    assert os.write(self.release_write, b'R') == 1

  def close(self) -> None:
    for descriptor in self.descriptors:
      os.close(descriptor)
    self.descriptors.clear()
