# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# HTTP/CNWB and silent-peer controls for the actual Navi bridge fixture.
from __future__ import annotations

import base64
import json
import time
from typing import TypedDict, assert_never

import anyio
from anyio.abc import SocketStream
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer
from carrot_server_live_wire import Wire


class Packet(TypedDict):
  metadata: dict[str, Json]
  payload: str


async def fetch(peer: Peer, path: str, method: str = 'GET', body: bytes = b'', local: str = '127.0.0.1') -> Wire:
  async with await anyio.connect_tcp('127.0.0.1', peer.ready['port'], local_host=local) as stream:
    await stream.send(f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {len(body)}\r\n\r\n'.encode() + body)
    with anyio.fail_after(3):
      return await read_response(BufferedByteReceiveStream(stream), method)


def payload(response: Wire) -> Json:
  return json.loads(base64.b64decode(response['body_base64']))


def stable(value: Json) -> Json:
  match value:
    case dict():
      return {key: stable(item) for key, item in value.items() if key not in {'ageMs', 'stateAgeMs', 'mapAgeMs', 'keyframeAgeMs'}}
    case list():
      return [stable(item) for item in value]
    case None | bool() | int() | float() | str():
      return value
    case unreachable:
      assert_never(unreachable)


def packet(raw: bytes) -> Packet:
  assert raw[:5] == b'CNWB\x01'
  size = int.from_bytes(raw[5:9], 'big')
  return {'metadata': json.loads(raw[9 : 9 + size]), 'payload': base64.b64encode(raw[9 + size :]).decode()}


async def raw_frame(reader: BufferedByteReceiveStream) -> tuple[int, bytes]:
  header = await reader.receive_exactly(2)
  size = header[1] & 127
  if size == 126:
    size = int.from_bytes(await reader.receive_exactly(2), 'big')
  if size == 127:
    size = int.from_bytes(await reader.receive_exactly(8), 'big')
  assert not header[1] & 128
  return header[0] & 15, await reader.receive_exactly(size)


async def upgrade(stream: SocketStream, path: str) -> BufferedByteReceiveStream:
  await stream.send(
    f'GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n'.encode()
    + b'Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: b3duZWQtc2lsZW50LXNvYw==\r\n\r\n'
  )
  reader = BufferedByteReceiveStream(stream)
  with anyio.fail_after(3):
    header = await reader.receive_until(b'\r\n\r\n', 65536)
    opcode, accepted = await raw_frame(reader)
  assert header.startswith(b'HTTP/1.1 101') and opcode == 1 and json.loads(accepted)['status'] == 'accepted'
  return reader


async def acknowledge(stream: SocketStream, payload: bytes) -> None:
  mask = b'\x13\x37\x00\x42'
  assert len(payload) < 126
  await stream.send(bytes((0x88, 0x80 | len(payload))) + mask + bytes(value ^ mask[index % 4] for index, value in enumerate(payload)))


async def silent_busy(peer: Peer) -> dict[str, Json]:
  async with await anyio.connect_tcp('127.0.0.1', peer.ready['port']) as stream:
    await stream.send(
      b'GET /ws/carrot_navi/state?client_id=silent-foreign HTTP/1.1\r\nHost: localhost\r\n'
      + b'Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n'
      + b'Sec-WebSocket-Key: b3duZWQtc2lsZW50LXNvYw==\r\n\r\n'
    )
    reader = BufferedByteReceiveStream(stream)
    with anyio.fail_after(3):
      header = await reader.receive_until(b'\r\n\r\n', 65536)
      opcode, status = await raw_frame(reader)
      close_opcode, close = await raw_frame(reader)
    assert header.startswith(b'HTTP/1.1 101') and opcode == 1 and close_opcode == 8
    started = time.monotonic()
    eof = False
    extra = []
    with anyio.move_on_after(11.5):
      try:
        while True:
          extra.append((await reader.receive()).hex())
      except anyio.EndOfStream:
        eof = True
    return {
      'session': json.loads(status),
      'close_code': int.from_bytes(close[:2], 'big'),
      'close_reason': close[2:].decode(),
      'eof': eof,
      'seconds': time.monotonic() - started,
      'extra_bytes': extra,
    }
