# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Loopback RTMPS ingress distinguishing TCP/health probes from real FFmpeg publish sessions."""

from __future__ import annotations

from pathlib import Path
import select
import socket
import ssl
import subprocess
import threading
from typing import TypedDict

from carrot_server_dashcam_upload import save
from carrot_server_youtube_recipient import Recipient


class Capture(TypedDict):
  purpose: str
  error: str
  tcp_bytes: int
  eof: bool


class Ingest:
  def __init__(self, output: Path, certificate: tuple[Path, Path]) -> None:
    self.output = output
    self.context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    self.context.load_cert_chain(*certificate)
    self.listener = socket.socket()
    self.listener.bind(('127.0.0.1', 0))
    self.listener.listen()
    self.listener.settimeout(0.1)
    self.port = self.listener.getsockname()[1]
    self.endpoint = f'rtmps://127.0.0.1:{self.port}/live2'
    self.stop = threading.Event()
    self.lock = threading.Lock()
    self.sockets: list[socket.socket] = []
    self.workers: list[threading.Thread] = []
    self.receivers: list[Recipient] = []
    self.observations: list[Capture] = []
    self.errors: list[str] = []
    self.thread = threading.Thread(target=self.run, name='owned-youtube-ingest')
    self.thread.start()

  def retain(self, source: socket.socket) -> None:
    with self.lock:
      self.sockets.append(source)

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
      self.retain(raw)
      worker = threading.Thread(target=self.forward, args=(raw,), name='owned-youtube-forward')
      self.workers.append(worker)
      worker.start()

  def forward(self, raw: socket.socket) -> None:
    capture: Capture = {'purpose': 'tcp-probe', 'error': '', 'tcp_bytes': 0, 'eof': False}
    sources: list[socket.socket] = [raw]
    receiver = None
    try:
      raw.settimeout(10)
      if not raw.recv(1, socket.MSG_PEEK):
        capture['eof'] = True
        return
      tls = self.context.wrap_socket(raw, server_side=True, do_handshake_on_connect=False)
      sources.append(tls)
      self.retain(tls)
      tls.do_handshake()
      capture['purpose'] = 'tls-health'
      first = tls.recv(65536)
      if not first:
        capture['eof'] = True
        return
      capture['purpose'] = 'rtmp-publish'
      assert first[0] == 3
      with self.lock:
        directory = self.output / f'session-{len(self.receivers)}'
        directory.mkdir()
        receiver = Recipient(directory)
        self.receivers.append(receiver)
      target = socket.create_connection(('127.0.0.1', receiver.port), timeout=3)
      sources.append(target)
      self.retain(target)
      target.settimeout(10)
      target.sendall(first)
      while not self.stop.is_set():
        ready, _writable, _errors = select.select([tls, target], [], [], 0.1)
        for source in ready:
          data = source.recv(65536)
          if not data:
            capture['eof'] = True
            return
          if source is tls:
            capture['tcp_bytes'] += len(data)
          (target if source is tls else tls).sendall(data)
    except (OSError, subprocess.SubprocessError, AssertionError) as error:
      capture['error'] = str(error)
    finally:
      for source in sources:
        source.close()
      if receiver:
        receiver.close()
        save(
          receiver.path.parent / 'recipient.json',
          {
            'argv': receiver.argv,
            'pid': receiver.process.pid,
            'exit': receiver.process.returncode,
            'bytes': receiver.path.stat().st_size if receiver.path.exists() else 0,
          },
        )
      with self.lock:
        self.observations.append(capture)

  def close(self) -> None:
    self.stop.set()
    self.listener.close()
    self.thread.join(timeout=2)
    with self.lock:
      sockets = self.sockets.copy()
    for source in sockets:
      try:
        source.shutdown(socket.SHUT_RDWR)
      except OSError as error:
        self.errors.append(str(error))
      source.close()
    for worker in self.workers:
      worker.join(timeout=4)
    assert not self.thread.is_alive() and all(not worker.is_alive() for worker in self.workers)
