from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
from typing import Any

from aiohttp import web

from .source import application, selected_source

HEADERS = ('content-type', 'content-length', 'allow')
WALL = 1700001000

def save(path: Path, value: Any) -> None:
  path.write_text(json.dumps(value, ensure_ascii=True, indent=2) + '\n')

def snapshot(path: Path) -> dict[str, Any]:
  result = {}
  for name, candidate in (('state', path), ('temporary', Path(str(path) + '.tmp'))):
    result[name] = {'kind': 'file', 'body_base64': base64.b64encode(candidate.read_bytes()).decode()} if candidate.is_file() else {'kind': 'directory' if candidate.is_dir() else 'missing'}
  return result

class Fixture:
  def __init__(self, binary: Path, output: Path, composed: bool) -> None:
    self.binary = binary; self.output = output; self.composed = composed
    self.root = output / 'owned-segments'; self.root.mkdir()
    self.states = [output / name / 'state.json' for name in ('source-state', 'native-state')]
    self.clock = [1.0]; self.observations = []; self.failures = []
    self.runner = None; self.native = None

  async def start(self) -> None:
    source, provenance = selected_source(self.root, self.states[0], self.clock, WALL)
    save(self.output / 'selected-original-functions.json', provenance)
    self.source = source
    self.runner = web.AppRunner(application(source, self.clock)); await self.runner.setup()
    site = web.TCPSite(self.runner, '127.0.0.1', 0); await site.start()
    self.source_port = site._server.sockets[0].getsockname()[1]
    self.native = await asyncio.create_subprocess_exec(str(self.binary), stdin=asyncio.subprocess.PIPE,
      stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    config = {'root': str(self.root), 'state': str(self.states[1]), 'wall': WALL,
      'monotonic': self.clock[0], 'composed': self.composed, 'app_state': str(self.output / 'owned-app-state')}
    self.native.stdin.write((json.dumps(config) + '\n').encode()); await self.native.stdin.drain()
    initial = await asyncio.wait_for(self.native.stdout.readline(), 10)
    if not initial: raise RuntimeError('native fixture startup failed')
    self.native_port = json.loads(initial)['port']
    save(self.output / 'owned-listeners.json', {'source': self.source_port, 'native': self.native_port, 'native_config': config})

  def marker(self, name: str, epoch: float, complete: bool = True) -> Path:
    directory = self.root / name; directory.mkdir(exist_ok=True)
    if complete: (directory / 'rlog.zst').write_bytes(b'owned metadata-only marker; never parsed')
    video = directory / 'qcamera.ts'; video.write_bytes(b'owned media metadata marker; never decoded')
    os.utime(video, (epoch, epoch)); return directory

  async def response(self, port: int, path: str, method: str, body: bytes | None, headers: dict[str, str]) -> dict[str, Any]:
    # Raw path keeps encoded static/dynamic boundaries visible to both routers.
    reader, writer = await asyncio.open_connection('127.0.0.1', port)
    request_headers = {'Host': '127.0.0.1', 'Connection': 'close', **headers}
    if body is not None: request_headers['Content-Length'] = str(len(body))
    request = f'{method} {path} HTTP/1.1\r\n' + ''.join(f'{key}: {value}\r\n' for key, value in request_headers.items()) + '\r\n'
    writer.write(request.encode('ascii') + (body or b'')); await writer.drain()
    data = await asyncio.wait_for(reader.read(), 10)
    writer.close(); await writer.wait_closed()
    head, body = data.split(b'\r\n\r\n', 1); lines = head.split(b'\r\n'); fields = {}
    for line in lines[1:]:
      key, value = line.split(b':', 1); fields[key.decode().lower()] = value.strip().decode('latin1')
    # All selected source responses carry a known Content-Length; no chunk parser is introduced.
    return {'status': int(lines[0].split()[1]), 'headers': {key: fields[key] for key in HEADERS if key in fields}, 'body_base64': base64.b64encode(body).decode()}

  async def pair(self, scenario: str, path: str, method: str = 'GET', body: bytes | None = None,
      headers: dict[str, str] | None = None, files: bool = False) -> None:
    headers = headers or {}
    expected = await self.response(self.source_port, path, method, body, headers)
    actual = await self.response(self.native_port, path, method, body, headers)
    row = {'scenario': scenario, 'request': {'method': method, 'path': path,
      'body_base64': base64.b64encode(body).decode() if body is not None else None, 'headers': headers},
      'source': expected, 'native': actual}
    if files: row['source_files'] = snapshot(self.states[0]); row['native_files'] = snapshot(self.states[1])
    row['equal'] = expected == actual and (not files or row['source_files'] == row['native_files'])
    self.observations.append(row)
    if not row['equal']: self.failures.append(row)

  async def control(self, scenario: str, **value: Any) -> None:
    await self.pair(scenario, '/__fixture/control', 'POST', json.dumps(value).encode(), {'Content-Type': 'application/json'})

  async def close(self) -> None:
    if self.native:
      self.native.stdin.write(b'\n'); await self.native.stdin.drain()
      stdout, stderr = await asyncio.wait_for(self.native.communicate(), 10)
      (self.output / 'native-stdout.txt').write_bytes(stdout); (self.output / 'native-stderr.txt').write_bytes(stderr)
    if self.runner: await self.runner.cleanup()

def prepare(binary: Path, output: Path, argv: list[str]) -> None:
  output.mkdir(parents=True, exist_ok=True)
  free = shutil.disk_usage(output).free; growth = 8 * 1024**2
  save(output / 'diskguard.json', {'free_bytes': free, 'estimated_growth_bytes': growth, 'reserve_bytes': 25 * 1024**3})
  if free < 25 * 1024**3 + growth: raise RuntimeError('disk reserve requires recovery')
  save(output / 'invocation.json', {'command': [sys.executable, '-P', *argv], 'PYTHONPATH': os.environ.get('PYTHONPATH', ''), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()})
  source_paths = [Path('openpilot/selfdrive/carrot/server/features/dashcam') / name for name in ('routes.py', 'catalog.py', 'paths.py', 'read_state.py')]
  save(output / 'source-hashes.json', {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in source_paths})
