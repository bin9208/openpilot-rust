# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned TLS health recipient counting actual connection purpose and cleanup."""

from __future__ import annotations

from pathlib import Path
import socket
import ssl
import threading
from typing import TypedDict


class Observation(TypedDict):
  tls: bool
  error: str


class Probe:
  def __init__(self, certificate: Path, key: Path) -> None:
    self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    self.context.load_cert_chain(certificate, key)
    self.listener = socket.socket()
    self.listener.bind(('127.0.0.1', 0))
    self.listener.listen()
    self.listener.settimeout(0.1)
    self.port = self.listener.getsockname()[1]
    self.endpoint = f'rtmps://127.0.0.1:{self.port}/live2'
    self.stop = threading.Event()
    self.observations: list[Observation] = []
    self.sockets: list[socket.socket] = []
    self.errors: list[str] = []
    self.thread = threading.Thread(target=self.run, name='owned-youtube-health')
    self.thread.start()

  def run(self) -> None:
    while not self.stop.is_set():
      try:
        raw, _address = self.listener.accept()
      except TimeoutError:
        continue
      except OSError as error:
        if not self.stop.is_set():
          self.errors.append(str(error))
        return
      self.sockets.append(raw)
      row: Observation = {'tls': False, 'error': ''}
      try:
        raw.settimeout(3)
        if not raw.recv(1, socket.MSG_PEEK):
          continue
        tls = self.context.wrap_socket(raw, server_side=True, do_handshake_on_connect=False)
        self.sockets.append(tls)
        tls.do_handshake()
        row['tls'] = True
      except OSError as error:
        row['error'] = str(error)
      finally:
        self.observations.append(row)
        for source in self.sockets:
          source.close()
        self.sockets.clear()

  def close(self) -> None:
    self.stop.set()
    for source in [self.listener, *self.sockets]:
      try:
        source.shutdown(socket.SHUT_RDWR)
      except OSError as error:
        self.errors.append(str(error))
      source.close()
    self.thread.join(timeout=2)
    assert not self.thread.is_alive()
