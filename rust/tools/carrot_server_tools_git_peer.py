from __future__ import annotations

import http.client
import json
import os
from pathlib import Path
import queue
import socket
import subprocess
import threading
import time

from carrot_server_git_status_peer import WRAPPER


class Peer:
  def __init__(self, command: list[str], root: Path, repository: Path, env: dict[str, str], launcher: Path, **options: bool):
    root.mkdir(parents=True)
    for directory in ('web', 'assets/training', 'data/state'):
      (root / directory).mkdir(parents=True)
    (root / 'web/plain.txt').write_bytes(b'owned health')
    (root / 'descriptor-probe').touch()
    (root / 'settings.json').write_text('{"menus":[]}')
    (root / 'data/state/git.json').write_text(json.dumps({'auto_update': {'status': 'owned', 'message': '한국어 </script>', 'count': 3}}))
    self.root = root
    self.trace = root / 'commands.jsonl'
    self.lock = root / 'repository.lock'
    self.descendant = root / 'descendant-pid'
    self.notice = root / 'notice'
    self.gate = root / 'gate'
    os.mkfifo(self.gate)
    wrapper = root / 'bin/git'
    wrapper.parent.mkdir()
    wrapper.write_text(WRAPPER)
    wrapper.chmod(0o700)
    self.env = env | {'PATH': str(wrapper.parent) + ':' + env['PATH'],
                      'CARROT_REPO_LOCK_PATH': str(self.lock), 'OWNED_GIT_TRACE': str(self.trace),
                      'OWNED_GIT_GATE': str(self.gate), 'OWNED_GIT_DESCENDANT': str(self.descendant)}
    if options.get('blocked'):
      self.env['OWNED_GIT_BLOCK'] = 'fetch'
    self.stderr = (root / 'stderr').open('w')
    self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=self.stderr, text=True, env=self.env)
    self.queue: queue.Queue[str | None] = queue.Queue()
    self.reader = threading.Thread(target=self._read, daemon=True)
    self.reader.start()
    config = {'root': str(root), 'repo': str(repository), 'lock': str(self.lock),
              'launcher': str(launcher), **options}
    self.process.stdin.write(json.dumps(config) + '\n')
    self.process.stdin.flush()
    self.started = time.monotonic()
    self.port = self.message()['port']
    selected_env = {key: value for key, value in self.env.items() if key.startswith(('OWNED_GIT_', 'GIT_CONFIG_', 'CARROT_REPO_')) or key in ('PATH', 'GIT_TERMINAL_PROMPT')}
    (root / 'invocation.json').write_text(json.dumps({'command': command, 'env': selected_env, 'config': config}, indent=2))
    self.observations: list[dict] = []

  def _read(self) -> None:
    for line in self.process.stdout:
      self.queue.put(line)
    self.queue.put(None)

  def message(self) -> dict:
    line = self.queue.get(timeout=15)
    if line is None:
      raise RuntimeError(f'owned peer exited: {self.root} code={self.process.poll()}')
    return json.loads(line)

  def calls(self) -> list[dict]:
    return [json.loads(line) for line in self.trace.read_text().splitlines()] if self.trace.exists() else []

  def request(self, path: str = '/api/tools/git_status', method: str = 'GET', headers: dict[str, str] | None = None) -> dict:
    connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=10)
    try:
      connection.request(method, path, headers=headers or {})
      response = connection.getresponse()
      body = response.read()
      row = {'method': method, 'path': path, 'status': response.status,
             'headers': {key: response.getheader(key) for key in ('Content-Type', 'Content-Length', 'Content-Encoding', 'Allow', 'Connection')},
             'body_hex': body.hex()}
      self.observations.append(row)
      return row
    finally:
      connection.close()

  def signal(self, command: str) -> None:
    self.process.stdin.write(command + '\n')
    self.process.stdin.flush()

  def error_connection(self) -> dict:
    request = f'GET /api/tools/git_status HTTP/1.1\r\nHost: 127.0.0.1:{self.port}\r\n\r\n'.encode()
    with socket.create_connection(('127.0.0.1', self.port), timeout=2) as connection:
      connection.sendall(request)
      wire = b''
      while b'\r\n\r\n' not in wire:
        chunk = connection.recv(8192)
        if not chunk:
          raise RuntimeError('owned error response closed before headers')
        wire += chunk
      header, body = wire.split(b'\r\n\r\n', 1)
      fields = dict(line.decode().split(': ', 1) for line in header.split(b'\r\n')[1:])
      length = int(next(value for name, value in fields.items() if name.lower() == 'content-length'))
      while len(body) < length:
        chunk = connection.recv(8192)
        if not chunk:
          raise RuntimeError('owned error response closed before body')
        body += chunk
        wire += chunk
      eof = connection.recv(1) == b''
    row = {'status': int(header.split(b' ')[1]), 'body_hex': body.hex(), 'eof': eof,
           'connection': next((value for name, value in fields.items() if name.lower() == 'connection'), None)}
    (self.root / 'error-connection.json').write_text(json.dumps(row | {'request_hex': request.hex(), 'response_hex': wire.hex()}, indent=2))
    return row

  def finish(self, started: float) -> dict:
    code = self.process.wait(timeout=12)
    self.reader.join(timeout=1)
    self.stderr.close()
    result = {'code': code, 'elapsed': time.monotonic() - started, 'last': self.message()}
    (self.root / 'observations.json').write_text(json.dumps(self.observations, indent=2))
    (self.root / 'stop.json').write_text(json.dumps(result, indent=2))
    if code != 0:
      raise RuntimeError(f'owned peer exit{code}: {self.root}')
    return result

  def stop(self, command: str = 'stop') -> dict:
    started = time.monotonic()
    self.signal(command)
    return self.finish(started)
