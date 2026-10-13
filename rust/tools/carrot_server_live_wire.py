# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Wire and compact-codec controls used by carrot_server_live.py and its App fixture.
from __future__ import annotations
import base64
import json
from pathlib import Path
from typing import TypedDict, assert_never

import anyio
from aiohttp import ClientWebSocketResponse, WSMsgType
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer
from carrot_server_dashcam_upload import save
from carrot_server_live_cases import Frame, frames


class Wire(TypedDict):
  status: int
  headers: dict[str, str]
  body_base64: str


async def fetch(peer: Peer, path: str, method: str = 'GET') -> Wire:
  async with await anyio.connect_tcp('127.0.0.1', peer.ready['port']) as stream:
    await stream.send(f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'.encode())
    with anyio.fail_after(3):
      return await read_response(BufferedByteReceiveStream(stream), method)


def normalize(value: Json) -> Json:
  match value:
    case dict():
      return {key: normalize(item) for key, item in value.items() if key not in {'generatedAtMs', 'snapshotAgeMs', 'ts'}}
    case list():
      return [normalize(item) for item in value]
    case None | bool() | int() | float() | str():
      return value
    case unexpected:
      assert_never(unexpected)


def body(response: Wire) -> Json:
  return json.loads(base64.b64decode(response['body_base64']))


async def binary_packet(ws: ClientWebSocketResponse) -> bytes:
  with anyio.fail_after(3):
    message = await ws.receive()
    assert message.type == WSMsgType.BINARY, (message.type, message.data)
    return message.data


async def codec(binary: Path, output: Path) -> list[Frame]:
  selected = frames(output / 'frames')
  process = await anyio.run_process([str(binary)], input=(json.dumps({'mode': 'codec', 'frames': selected}) + '\n').encode())
  actual = json.loads(process.stdout)
  save(output / 'codec-native.json', actual)
  expected = [{'service': row['service'], 'bytes': row['expected']} for row in selected]
  assert actual == expected
  save(output / 'codec-result.json', {'comparisons': len(selected), 'equal': True, 'input': selected})
  return selected
