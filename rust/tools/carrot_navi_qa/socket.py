from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from collections.abc import Awaitable, Callable
import json
import struct
from typing import TypeAlias, TypedDict

import aiohttp

from carrot_navi_cases import query

Json: TypeAlias = None | bool | int | float | str | list['Json'] | dict[str, 'Json']


class Row(TypedDict):
  action: str
  value: Json


def object_text(raw: str) -> dict[str, Json]:
  value = json.loads(raw)
  if not isinstance(value, dict):
    raise TypeError('expected receiver JSON object')
  return value


@dataclass(slots=True)
class Client:
  http: aiohttp.ClientSession
  url: str
  rows: list[Row] = field(default_factory=list)
  observer: Callable[[dict[str, Json]], Awaitable[None]] | None = None
  register_media: Callable[[str, bytes], None] | None = None

  async def http_read(self, path: str = '/health', method: str = 'GET') -> dict[str, Json]:
    async with self.http.request(method, self.url + path) as response:
      text = await response.text()
      body = json.loads(text) if response.content_type == 'application/json' and text else text
      self.rows.append({'action': f'{method} {path}', 'value': {'status': response.status,
        'content_type': response.headers.get('Content-Type'), 'body': body}})
      return body if isinstance(body, dict) else {}

  async def health_until(self, key: str, expected: Json) -> dict[str, Json]:
    deadline = asyncio.get_running_loop().time() + 5
    while True:
      async with self.http.get(self.url + '/health') as response:
        body = object_text(await response.text())
      if body.get(key) == expected:
        self.rows.append({'action': f'health {key}={expected}', 'value': body})
        if self.observer is not None and key in ('received_count', 'control_connections', 'session_received_count'):
          await self.observer(body)
        return body
      if asyncio.get_running_loop().time() >= deadline:
        raise TimeoutError(f'health {key} did not become {expected}: {body}')
      await asyncio.sleep(.001)

  async def response(self, ws: aiohttp.ClientWebSocketResponse, action: str) -> dict[str, Json]:
    message = await ws.receive(timeout=5)
    if message.type != aiohttp.WSMsgType.TEXT:
      raise RuntimeError(f'expected JSON reply, received {message.type}')
    body = object_text(message.data)
    self.rows.append({'action': action, 'value': body})
    return body

  async def rejected_stream(self, path: str, payload: str | bytes | None = None) -> None:
    async with self.http.ws_connect(self.url + path, headers={'X-Forwarded-For': 'owned, proxy'}) as ws:
      if isinstance(payload, str):
        await ws.send_str(payload)
      elif isinstance(payload, bytes):
        await ws.send_bytes(payload)
      await self.response(ws, path)
      message = await ws.receive(timeout=5)
      if message.type != aiohttp.WSMsgType.CLOSE:
        raise RuntimeError(f'expected policy close: {message.type}')
      self.rows.append({'action': 'stream policy close', 'value': {'code': message.data, 'reason': message.extra}})

  async def normal(self) -> None:
    for path in ('/', '/health', '/api/navi/latest', '/missing'):
      await self.http_read(path)
    await self.http_read('/health', 'POST')
    await self.http_read('/', 'HEAD')
    await self.http_read('/api/navi/ws/v2/control/not-upgraded')
    async with self.http.ws_connect(self.url + '/api/navi/ws/v2/control/owned%20app',
      headers={'X-Forwarded-For': 'owned, proxy'}) as control:
      await self.health_until('control_connections', 1)
      for invalid in (b'wrong type', '{', '[]', '{"type":"requirements_query","protocol_version":2,"catalog_revision":0}'):
        if isinstance(invalid, bytes):
          await control.send_bytes(invalid)
        else:
          await control.send_str(invalid)
        await self.response(control, 'control recoverable')
      await control.send_str(json.dumps(query()))
      manifest = await self.response(control, 'manifest')
      session = manifest['session_id']
      if not isinstance(session, str) or len(session) != 16:
        raise ValueError('invalid session token')
      streams = manifest['streams']
      if not isinstance(streams, list):
        raise TypeError('manifest streams must be list')
      count = 0
      for stream in streams:
        if not isinstance(stream, dict):
          raise TypeError('manifest entry must be object')
        kind, name, handle = stream['kind'], stream['name'], stream['stream_handle']
        path = f'/api/navi/ws/v2/{kind}/{session}/{name}'
        if not stream['enabled']:
          await self.rejected_stream(path)
          continue
        async with self.http.ws_connect(self.url + path, headers={'X-Forwarded-For': 'owned, proxy'}) as ws:
          if kind == 'json':
            value: Json = [{'count': 3}] if name == 'lane_ahead' else {'test': name, 'foreground': False}
            update = {'type': 'item_update', 'protocol_version': 2, 'session_id': session,
              'kind': kind, 'name': name, 'schema_version': 1, 'stream_handle': handle,
              'manifest_revision': 1, 'sequence': 1, 'source_timestamp_ms': 1001,
              'sent_at_ms': 1002, 'present': True, 'value': value}
            await ws.send_str(json.dumps(update))
          else:
            payload = b'\xff\xd8content\xff\xd9' if kind == 'render' else b'\x89PNG\r\n\x1a\ncontent'
            if self.register_media is not None:
              self.register_media(f'{kind}:{name}', payload)
            header = struct.pack('>4sBBBBIIQQIHH', b'CNV2', 2, 1, 2 if kind == 'render' else 1,
              0, handle, 1, 1, 1001, len(payload), 32, 24)
            await ws.send_bytes(header + payload)
          count += 1
          await self.health_until('received_count', count)
      await self.http_read('/api/navi/latest')
      await self.rejected_stream(f'/api/navi/ws/v2/json/{session}/vehicle', '{}')
      await self.rejected_stream(f'/api/navi/ws/v2/image/{session}/tbt_next', b'bad')
      await self.rejected_stream(f'/api/navi/ws/v2/render/{session}/map_main', 'wrong type')
      await self.rejected_stream('/api/navi/ws/v2/json/stale/vehicle')
      await control.send_str(json.dumps({'protocol_version': 2, 'type': 'metrics', 'value': 'kept'}))
      await self.health_until('control_event_count', 1)
      await control.send_str(json.dumps(query()))
      await self.response(control, 'renegotiated manifest')
      await self.health_until('session_received_count', 0)
    await self.health_until('control_connections', 0)
    await self.http_read('/api/navi/latest')
