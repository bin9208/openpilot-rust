from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import socket
import ssl
import threading
import time


@dataclass(frozen=True, slots=True)
class Response:
  wire: bytes
  hold: bool = False
  parts: tuple[bytes, ...] = ()
  interval: float = 0


def response(body: bytes = b'owned', status: int = 200, headers: tuple[tuple[str, str], ...] = (), length: int | None = None) -> Response:
  header = f'HTTP/1.1 {status} Owned\r\nConnection: close\r\nContent-Length: {len(body) if length is None else length}\r\n'
  header += ''.join(f'{name}: {value}\r\n' for name, value in headers)
  return Response(header.encode() + b'\r\n' + body)


class Peer:
  def __init__(self, responses: tuple[Response, ...], certificate: tuple[Path, Path] | None = None):
    self.responses = responses
    self.socket = socket.socket()
    self.socket.bind(('127.0.0.1', 0))
    self.socket.listen()
    self.socket.settimeout(0.2)
    self.address = self.socket.getsockname()
    self.received = threading.Event()
    self.release = threading.Event()
    self.closed = threading.Event()
    self.rows = []
    self.errors = []
    self.completed = []
    self.threads = []
    self.tls = None
    if certificate:
      self.tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
      self.tls.load_cert_chain(*certificate)
    self.thread = threading.Thread(target=self.accept, daemon=True)
    self.thread.start()

  @property
  def endpoint(self) -> str:
    return f'{"https" if self.tls else "http"}://{self.address[0]}:{self.address[1]}/carrot/api_heartbeat.php'

  def accept(self) -> None:
    while not self.closed.is_set():
      try:
        connection, _ = self.socket.accept()
      except TimeoutError:
        continue
      except OSError:
        return
      thread = threading.Thread(target=self.exchange, args=(connection,), daemon=True)
      self.threads.append(thread)
      thread.start()

  def exchange(self, connection: socket.socket) -> None:
    try:
      connection.settimeout(5)
      if self.tls:
        connection = self.tls.wrap_socket(connection, server_side=True)
      with connection:
        data = b''
        while b'\r\n\r\n' not in data:
          part = connection.recv(65536)
          if not part:
            return
          data += part
        head, body = data.split(b'\r\n\r\n', 1)
        lines = head.decode('latin1').split('\r\n')
        headers = dict(line.split(':', 1) for line in lines[1:])
        headers = {name.lower(): value.strip() for name, value in headers.items()}
        length = int(headers.get('content-length', '0'))
        while len(body) < length:
          part = connection.recv(length-len(body))
          if not part:
            raise EOFError('owned request incomplete')
          body += part
        index = len(self.rows)
        self.rows.append(dict(request_line=lines[0], headers=headers, body=body.hex(), received=time.monotonic()))
        self.received.set()
        reply = self.responses[min(index, len(self.responses)-1)]
        if reply.hold and not self.release.wait(40):
          raise TimeoutError('owned response gate not released')
        connection.sendall(reply.wire)
        for part in reply.parts:
          time.sleep(reply.interval)
          connection.sendall(part)
        self.completed.append(index)
    except (OSError, EOFError) as error:
      self.errors.append(dict(type=type(error).__name__, message=str(error)))
    finally:
      connection.close()

  def close(self) -> None:
    self.release.set()
    self.closed.set()
    self.socket.close()
    self.thread.join(1)
    for thread in self.threads:
      thread.join(1)
