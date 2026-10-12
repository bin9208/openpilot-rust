from __future__ import annotations

import asyncio
import base64
from typing import Any

from .fixture import Fixture, save

async def read_response(reader: asyncio.StreamReader) -> dict[str, Any]:
  header = await asyncio.wait_for(reader.readuntil(b'\r\n\r\n'), 5)
  lines = header.split(b'\r\n'); fields = {}
  for line in lines[1:]:
    if not line: continue
    key, value = line.split(b':', 1); fields[key.decode().lower()] = value.strip().decode('latin1')
  body = await asyncio.wait_for(reader.readexactly(int(fields['content-length'])), 5)
  return {'status': int(lines[0].split()[1]), 'headers': {key: fields[key] for key in ('content-type', 'content-length', 'connection') if key in fields}, 'all_headers': fields, 'body_base64': base64.b64encode(body).decode()}

async def observe(port: int) -> dict[str, Any]:
  reader, writer = await asyncio.open_connection('127.0.0.1', port)
  payload = b'{"recentSegment":"owned--6"}'
  request = (f'POST /api/dashcam/read-state HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {len(payload)}\r\n\r\n').encode() + payload
  writer.write(request); await writer.drain()
  response = await read_response(reader)
  eof = False; probe = None
  try:
    probe = await asyncio.wait_for(reader.read(1), .25); eof = probe == b''
  except TimeoutError: pass
  follow = None; follow_error = None
  try:
    writer.write(b'GET /api/dashcam/read-state HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n'); await writer.drain()
    follow = await read_response(reader)
  except (asyncio.IncompleteReadError, ConnectionError, TimeoutError) as error:
    follow_error = {'kind': type(error).__name__, 'text': str(error)}
  finally:
    writer.close()
    try: await writer.wait_closed()
    except ConnectionError: pass
  return {'request_base64': base64.b64encode(request).decode(), 'response': response, 'eof_after_response': eof,
    'probe_base64': base64.b64encode(probe).decode() if probe is not None else None,
    'follow_up': follow, 'follow_up_error': follow_error}

async def run(fixture: Fixture) -> None:
  for path in fixture.states: path.parent.mkdir(); path.mkdir()
  expected = await observe(fixture.source_port); actual = await observe(fixture.native_port)
  def observable(value: dict[str, Any]) -> dict[str, Any]:
    return {'response': {key: value['response'][key] for key in ('status', 'headers', 'body_base64')},
      'eof_after_response': value['eof_after_response'],
      'follow_up_status': value['follow_up']['status'] if value['follow_up'] else None}
  row = {'scenario': 'read-state-replace-error-default-http11-keepalive', 'source': expected, 'native': actual,
    'compared_source': observable(expected), 'compared_native': observable(actual), 'equal': observable(expected) == observable(actual)}
  fixture.observations.append(row)
  if not row['equal']: fixture.failures.append(row)
  save(fixture.output / 'keepalive-socket-observation.json', row)
  for path in fixture.states: path.rmdir()
  await fixture.pair('replace-error-recovery-on-fresh-connection', '/api/dashcam/read-state', 'POST', b'{"recentSegment":"owned--7"}', {'Content-Type': 'application/json'}, files=True)
