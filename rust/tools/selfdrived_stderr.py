import os
import select
import time
from typing import BinaryIO


def wait_for_stderr(stream: BinaryIO, marker: bytes, captured: bytearray, timeout: float = 5) -> None:
  deadline = time.monotonic() + timeout
  while marker not in captured:
    remaining = deadline - time.monotonic()
    assert remaining > 0 and select.select([stream], [], [], remaining)[0], 'waiting startup never logged readiness'
    chunk = os.read(stream.fileno(), 4096)
    assert chunk, 'stderr closed before startup readiness'
    captured.extend(chunk)
