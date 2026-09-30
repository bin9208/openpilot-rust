"""Loopback receiver recording real chunk framing and request payloads."""

from __future__ import annotations

import base64
import hashlib
import json
import threading
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TypeAlias

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


@dataclass(slots=True)
class Fixture:
  """Mutable request capture owned by a single, sequential scenario."""

  output: Path
  plans: list[dict[str, Json]] = field(default_factory=list)
  captures: list[dict[str, Json]] = field(default_factory=list)
  done: threading.Condition = field(default_factory=threading.Condition)
  prefix: str = ''

  def record(self, capture: dict[str, Json], body: bytes) -> dict[str, Json]:
    with self.done:
      index = len(self.captures)
      artifact = self.output / f'{self.prefix}-{index:02d}.body'
      if body:
        artifact.write_bytes(body)
      else:
        artifact = artifact.with_suffix('.empty.json')
        artifact.write_text(json.dumps({'body_base64': base64.b64encode(body).decode()}))
      capture.update(body_path=str(artifact), body_sha256=hashlib.sha256(body).hexdigest(), body_size=len(body))
      self.captures.append(capture)
      self.done.notify_all()
      return self.plans[min(index, len(self.plans) - 1)] if self.plans else {}


def server_for(fixture: Fixture) -> ThreadingHTTPServer:
  class Handler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def handle(self) -> None:
      try:
        super().handle()
      except (ConnectionResetError, BrokenPipeError):
        self.close_connection = True

    def log_message(self, *_args: str) -> None:
      return

    def do_GET(self) -> None:
      self.respond()

    def do_POST(self) -> None:
      self.respond()

    def do_PUT(self) -> None:
      self.respond()

    def respond(self) -> None:
      body = bytearray()
      chunks: list[int] = []
      complete = True
      if self.headers.get('Transfer-Encoding') == 'chunked':
        while True:
          line = self.rfile.readline()
          if not line:
            complete = False
            break
          size = int(line.split(b';', 1)[0], 16)
          if size == 0:
            self.rfile.readline()
            break
          chunk = self.rfile.read(size)
          body.extend(chunk)
          chunks.append(len(chunk))
          if len(chunk) != size or self.rfile.read(2) != b'\r\n':
            complete = False
            break
      else:
        body.extend(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
      capture: dict[str, Json] = {'method': self.command, 'path': self.path, 'headers': dict(self.headers), 'chunks': chunks, 'complete': complete}
      plan = fixture.record(capture, bytes(body))
      if not complete or plan.get('disconnect'):
        self.close_connection = True
        return
      status = int(str(plan.get('status', 200)))
      if 'text' in plan:
        data = str(plan['text']).encode()
      else:
        payload = plan.get('body', {'ok': True, 'token': 'synthetic-session', 'size': len(body) + int(str(plan.get('size_delta', 0)))})
        data = json.dumps(payload).encode()
      if plan.get('delay_headers'):
        threading.Event().wait(float(plan['delay_headers']))
      self.send_response(status)
      self.send_header('Content-Type', 'application/json; charset=utf-8')
      self.send_header('Content-Length', str(0 if status == 204 else len(data)))
      if plan.get('location'):
        self.send_header('Location', str(plan['location']))
      self.end_headers()
      try:
        if status != 204:
          pieces = int(str(plan.get('response_pieces', 1)))
          width = max(1, (len(data) + pieces - 1) // pieces)
          for start in range(0, len(data), width):
            if plan.get('delay_body'):
              threading.Event().wait(float(plan['delay_body']))
            self.wfile.write(data[start : start + width])
            self.wfile.flush()
      except (BrokenPipeError, ConnectionResetError):
        self.close_connection = True

  return ThreadingHTTPServer(('127.0.0.1', 0), Handler)


def wire(captures: list[dict[str, Json]]) -> list[dict[str, Json]]:
  result: list[dict[str, Json]] = []
  for capture in captures:
    headers = {str(key).lower(): value for key, value in dict(capture['headers']).items()}
    body_path = Path(str(capture['body_path']))
    body = base64.b64decode(json.loads(body_path.read_text())['body_base64']) if body_path.suffix == '.json' else body_path.read_bytes()
    content_type = str(headers.get('content-type', ''))
    if content_type.startswith('multipart/form-data; boundary='):
      boundary = content_type.split('boundary=', 1)[1]
      body = body.replace(boundary.encode(), b'BOUNDARY')
      content_type = 'multipart/form-data; boundary=BOUNDARY'
    result.append(
      {
        'method': capture['method'],
        'path': capture['path'],
        'body_sha256': hashlib.sha256(body).hexdigest(),
        'body_size': len(body),
        'chunked': bool(capture['chunks']) or str(headers.get('transfer-encoding', '')) == 'chunked',
        'complete': capture['complete'],
        'content_type': content_type,
        'authorization': headers.get('authorization'),
        'file_size': headers.get('x-file-size'),
      }
    )
  return result
