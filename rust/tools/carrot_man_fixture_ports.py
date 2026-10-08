from collections.abc import Iterator
from contextlib import ExitStack, contextmanager
import socket


@contextmanager
def reserve_ports() -> Iterator[list[int]]:
  with ExitStack() as reservations:
    ports: list[int] = []
    for _ in range(7):
      stream = reservations.enter_context(socket.socket(type=socket.SOCK_STREAM))
      stream.bind(("127.0.0.1", 0))
      ports.append(stream.getsockname()[1])
    yield ports
