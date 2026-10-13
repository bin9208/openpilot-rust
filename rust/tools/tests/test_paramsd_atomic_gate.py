from __future__ import annotations

from pathlib import Path
import os
import select
import struct
import sys

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from paramsd_startup_gate import StartupGate


def gate_for_cursor(tmp_path: Path, cursor: int) -> StartupGate:
  queue = tmp_path / 'livePose'
  with queue.open('wb') as file:
    file.truncate(256984)
    file.write(struct.pack('<2Q', 1, cursor))
  return StartupGate(tmp_path / 'position', queue, tmp_path)


@pytest.mark.parametrize('previous,committed', [(0, 232), (255896, 4294967528), (0xFFFFFFFF0003E798, 232)])
def test_release_accepts_complete_pose_commit(tmp_path: Path, previous: int, committed: int) -> None:
  # Given a complete 224-byte pose payload committed to a normal or wrapped queue.
  gate = gate_for_cursor(tmp_path, committed)
  try:
    # When the fixture releases the input barrier.
    gate.release(previous, 224)
    # Then one release token and a verified batch are recorded.
    assert select.select([gate.release_read], [], [], 0)[0]
    assert os.read(gate.release_read, 1) == b'R'
    assert gate.report['batches_queued_before_release'] == 1
  finally:
    gate.close()


def test_wrap_header_alone_does_not_release_reader(tmp_path: Path) -> None:
  # Given the intermediate wrap cursor with no committed payload in the new cycle.
  gate = gate_for_cursor(tmp_path, 4294967296)
  try:
    # When the fixture receives only that header transition.
    with pytest.raises(AssertionError):
      gate.release(255896, 224)
    # Then no release is sent to the native reader.
    assert not select.select([gate.release_read], [], [], 0)[0]
    assert gate.report['batches_queued_before_release'] == 0
  finally:
    gate.close()


def test_unrelated_pose_size_does_not_release_reader(tmp_path: Path) -> None:
  # Given a complete cursor for a different payload length.
  gate = gate_for_cursor(tmp_path, 4294967520)
  try:
    # When the current 224-byte pose is expected.
    with pytest.raises(AssertionError):
      gate.release(255896, 224)
    # Then the wrong committed cursor cannot authorize input processing.
    assert not select.select([gate.release_read], [], [], 0)[0]
  finally:
    gate.close()
