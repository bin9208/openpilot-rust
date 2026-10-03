import os
from pathlib import Path
import subprocess
import sys
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from selfdrived_stderr import wait_for_stderr


def check_script(script: str, expected: bytes) -> None:
  with subprocess.Popen([sys.executable, '-c', script], stderr=subprocess.PIPE) as process:
    captured = bytearray()
    wait_for_stderr(process.stderr, b'waiting for CarParams', captured, timeout=5)
    _, remaining = process.communicate(timeout=5)
    assert bytes(captured) + remaining == expected
    assert process.returncode == 0


def test_git_diagnostic_and_marker_in_one_write() -> None:
  expected = b'fatal: HEAD does not point to a branch\nselfdrived is waiting for CarParams\ntail\n'
  check_script(f'import os; os.write(2, {expected!r})', expected)


def test_split_marker_after_delayed_diagnostic() -> None:
  check_script(
    "import os,time; os.write(2,b'diagnostic\\nwaiting for Car'); time.sleep(0.02); os.write(2,b'Params\\n')",
    b'diagnostic\nwaiting for CarParams\n',
  )


def test_eof_keeps_diagnostic() -> None:
  with subprocess.Popen([sys.executable, '-c', "import os; os.write(2,b'fatal startup\\n')"], stderr=subprocess.PIPE) as process:
    captured = bytearray()
    with pytest.raises(AssertionError, match='closed before'):
      wait_for_stderr(process.stderr, b'waiting for CarParams', captured, timeout=5)
    assert captured == b'fatal startup\n'
    process.communicate(timeout=5)


def test_timeout_keeps_diagnostic() -> None:
  read_fd, write_fd = os.pipe()
  try:
    os.write(write_fd, b'still starting\n')
    with os.fdopen(read_fd, 'rb') as stream:
      captured = bytearray()
      with pytest.raises(AssertionError, match='never logged readiness'):
        wait_for_stderr(stream, b'waiting for CarParams', captured, timeout=0.02)
      assert captured == b'still starting\n'
  finally:
    os.close(write_fd)
