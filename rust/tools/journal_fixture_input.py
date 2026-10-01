from collections.abc import Iterator
from contextlib import contextmanager
import os
import select
import signal
from types import FrameType


class Terminated(Exception):
  def __init__(self, signum: int):
    self.signum = signum
    super().__init__(signum)


def wake_handler(signum: int, _frame: FrameType | None) -> None:
  # Unwind buffered writes before the main loop records termination; never reenter trace I/O.
  raise Terminated(signum)


def read_lines(wake_fd: int) -> Iterator[bytes]:
  pending = bytearray()
  while True:
    readable, _, _ = select.select([0, wake_fd], [], [])
    if wake_fd in readable:
      raise Terminated(os.read(wake_fd, 4096)[0])
    chunk = os.read(0, 8192)
    if not chunk:
      if pending:
        yield bytes(pending)
      return
    pending.extend(chunk)
    while b'\n' in pending:
      line, _, rest = pending.partition(b'\n')
      pending = bytearray(rest)
      yield bytes(line)


@contextmanager
def input_lines() -> Iterator[Iterator[bytes]]:
  read_fd, write_fd = os.pipe2(os.O_NONBLOCK | os.O_CLOEXEC)
  with os.fdopen(read_fd, 'rb', buffering=0), os.fdopen(write_fd, 'wb', buffering=0):
    previous_handler = signal.signal(signal.SIGTERM, wake_handler)
    previous_wakeup = signal.set_wakeup_fd(write_fd)
    try:
      yield read_lines(read_fd)
    finally:
      signal.set_wakeup_fd(previous_wakeup)
      signal.signal(signal.SIGTERM, previous_handler)
