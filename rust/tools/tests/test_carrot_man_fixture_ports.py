from contextlib import ExitStack
from types import TracebackType
from typing import Self
import errno
import socket
import unittest
from unittest.mock import patch

import carrot_man_fixture_ports as subject


REAL_SOCKET = socket.socket


def recurring_socket_factory():
  with REAL_SOCKET() as probe:
    probe.bind(('127.0.0.1', 0))
    candidate = probe.getsockname()[1]
  streams: list[socket.socket] = []

  class Reservation:
    def __init__(self, *, type: int = socket.SOCK_STREAM) -> None:
      self.stream = REAL_SOCKET(type=type)
      streams.append(self.stream)

    def __enter__(self) -> Self:
      return self

    def __exit__(self, exc_type: type[BaseException] | None, exc: BaseException | None,
                 traceback: TracebackType | None) -> None:
      self.stream.close()

    def bind(self, address: tuple[str, int]) -> None:
      held = any(stream.fileno() >= 0 and stream.getsockname()[1] == candidate for stream in streams)
      self.stream.bind((address[0], candidate if address[1] == 0 and not held else address[1]))

    def getsockname(self) -> tuple[str, int]:
      return self.stream.getsockname()

  return Reservation


class FixturePortsTest(unittest.TestCase):
  def test_ports_stay_distinct_when_the_kernel_can_reuse_a_released_candidate(self) -> None:
    # Given: a recurring ephemeral candidate with real bound sockets.
    with patch.object(subject.socket, 'socket', recurring_socket_factory()):
      # When: the fixture allocates its complete listener set.
      with subject.reserve_ports() as ports:
        # Then: all seven ports remain distinct and reserved until release.
        self.assertEqual(len(ports), 7)
        self.assertEqual(len(set(ports)), 7)
        for port in ports:
          with REAL_SOCKET() as listener:
            with self.assertRaises(OSError) as error:
              listener.bind(('127.0.0.1', port))
            self.assertEqual(error.exception.errno, errno.EADDRINUSE)
      with ExitStack() as listeners:
        for port in ports:
          listeners.enter_context(REAL_SOCKET()).bind(('127.0.0.1', port))

  def test_ports_are_released_when_fixture_preparation_fails(self) -> None:
    # Given: a fixture that fails after allocating listeners but before launch.
    ports: list[int] = []
    with self.assertRaisesRegex(RuntimeError, 'fixture preparation failed'):
      with subject.reserve_ports() as ports:
        # When: fixture preparation raises before it can launch a child.
        raise RuntimeError('fixture preparation failed')
    # Then: every reservation can be bound by the subsequent listener.
    with ExitStack() as listeners:
      for port in ports:
        listeners.enter_context(REAL_SOCKET()).bind(('127.0.0.1', port))


if __name__ == '__main__':
  unittest.main()
