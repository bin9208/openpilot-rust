# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Owned FFmpeg RTMP recipient and optional TLS forwarding for boundary comparisons."""

from __future__ import annotations

from pathlib import Path
import select
import socket
import ssl
import subprocess
import threading
import time
from types import TracebackType


def port() -> int:
  with socket.socket() as listener:
    listener.bind(('127.0.0.1', 0))
    return listener.getsockname()[1]


class RecipientError(Exception):
  """Owned FFmpeg exited before its listening boundary became available."""


class Recipient:
  def __init__(self, output: Path) -> None:
    self.port = port()
    self.url = f'rtmp://127.0.0.1:{self.port}/live2/owned-stream-key'
    self.path = output / 'recipient.flv'
    self.argv = ['ffmpeg', '-hide_banner', '-loglevel', 'warning', '-listen', '1', '-timeout', '12', '-i', self.url, '-c', 'copy', '-f', 'flv', str(self.path)]
    self.log = (output / 'recipient.log').open('wb')
    self.process = subprocess.Popen(self.argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=self.log)
    ready = False
    try:
      deadline = time.monotonic() + 3
      while self.process.poll() is None:
        if any(row.split()[1].endswith(f':{self.port:04X}') and row.split()[3] == '0A' for row in Path('/proc/net/tcp').read_text().splitlines()[1:]):
          break
        if time.monotonic() >= deadline:
          raise TimeoutError('owned RTMP recipient did not listen')
        time.sleep(0.01)
      else:
        raise RecipientError('owned RTMP recipient exited before listening')
      ready = True
    finally:
      if not ready:
        try:
          self.close()
        except (OSError, subprocess.TimeoutExpired) as cleanup:
          self.cleanup_error = str(cleanup)

  def close(self) -> None:
    try:
      try:
        self.process.wait(timeout=2)
      except subprocess.TimeoutExpired:
        self.process.terminate()
        try:
          self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
          self.process.kill()
          self.process.wait(timeout=2)
    finally:
      self.log.close()

  def __enter__(self) -> Recipient:
    return self

  def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, trace: TracebackType | None) -> None:
    self.close()


class TlsPeer:
  def __init__(self, certificate: Path, key: Path, target: int | None, *, trickle: bool = False) -> None:
    self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    self.context.load_cert_chain(certificate, key)
    self.target = target
    self.trickle = trickle
    self.listener = socket.socket()
    self.listener.bind(('127.0.0.1', 0))
    self.listener.listen(1)
    self.port = self.listener.getsockname()[1]
    self.stop = threading.Event()
    self.pause = threading.Event()
    self.paused = threading.Event()
    self.discard = threading.Event()
    self.sockets: list[socket.socket] = []
    self.error = ''
    self.shutdown_errors: list[str] = []
    self.eof = False
    self.trickled_bytes = 0
    self.started = time.monotonic()
    self.finished = 0.0
    self.thread = threading.Thread(target=self.run, name='owned-youtube-tls-peer')
    self.thread.start()

  def run(self) -> None:
    try:
      raw, _address = self.listener.accept()
      self.sockets.append(raw)
      raw.settimeout(11)
      if self.trickle:
        incoming, outgoing = ssl.MemoryBIO(), ssl.MemoryBIO()
        tls = self.context.wrap_bio(incoming, outgoing, server_side=True)
        incoming.write(raw.recv(65536))
        try:
          tls.do_handshake()
        except ssl.SSLWantReadError:
          assert outgoing.pending > 0
        server_hello = outgoing.read()
        assert len(server_hello) > 50
        raw.sendall(server_hello[:10])
        for byte in server_hello[10:]:
          readable, _writable, _errors = select.select([raw], [], [], 1)
          if readable and not raw.recv(65536):
            self.eof = True
            return
          if self.stop.is_set():
            return
          raw.sendall(bytes([byte]))
          self.trickled_bytes += 1
      else:
        tls = self.context.wrap_socket(raw, server_side=True, do_handshake_on_connect=False)
        self.sockets.append(tls)
        tls.do_handshake()
        if self.target is None:
          self.eof = not tls.recv(1)
          return
        with socket.create_connection(('127.0.0.1', self.target), timeout=3) as target:
          self.sockets.append(target)
          while not self.stop.is_set():
            if self.pause.is_set():
              self.paused.set()
              self.stop.wait(0.02)
              continue
            readable, _writable, _errors = select.select([tls, target], [], [], 0.1)
            for source in readable:
              data = source.recv(65536)
              if not data:
                self.eof = True
                return
              if not self.discard.is_set():
                (target if source is tls else tls).sendall(data)
    except OSError as error:
      self.error = str(error)
    finally:
      self.finished = time.monotonic()
      for source in self.sockets:
        source.close()

  def close(self) -> None:
    self.stop.set()
    for source in [self.listener, *self.sockets]:
      try:
        source.shutdown(socket.SHUT_RDWR)
      except OSError as error:
        self.shutdown_errors.append(str(error))
      source.close()
    self.thread.join(timeout=2)
    assert not self.thread.is_alive()
