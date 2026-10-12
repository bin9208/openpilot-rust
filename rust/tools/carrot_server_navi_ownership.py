# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Actual ownership/Params guard and profile effects, using cooperative WebSocket peers.
from __future__ import annotations

from contextlib import AsyncExitStack
from pathlib import Path
import sys
import time
from typing import TypedDict

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse, WSMsgType
from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_navi_cases import media
from carrot_server_navi_fixture import Fixture, fixture
from carrot_server_navi_http_cases import equal
from carrot_server_navi_streams import connect
from carrot_server_navi_wire import fetch, payload, raw_frame, upgrade


def wait_value(path: Path, expected: bytes) -> bytes:
  deadline = time.monotonic() + 2
  while time.monotonic() < deadline:
    if path.exists() and path.read_bytes() == expected:
      return expected
    time.sleep(0.005)
  raise TimeoutError(f'Params effect missing: {path.name}={expected!r}')


class CloseObservation(TypedDict):
  code: int
  reason: str


async def closed(ws: ClientWebSocketResponse, expected: int) -> CloseObservation:
  with anyio.fail_after(3):
    while True:
      message = await ws.receive()
      if message.type == WSMsgType.CLOSE:
        assert message.data == expected, message
        return {'code': message.data, 'reason': message.extra}
      assert message.type in (WSMsgType.TEXT, WSMsgType.BINARY), message


async def ownership(owned: Fixture, client: ClientSession, sockets: list[ClientWebSocketResponse], output: Path) -> None:
  records = []
  first = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=first')
  first_media = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=first&map=0')
  for path in ('/ws/carrot_navi/state?client_id=second', '/ws/carrot_navi/media?client_id=second&takeover=1'):
    responses = []
    for peer in owned.peers:
      ws = await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}' + path, compress=0)
      sockets.append(ws)
      with anyio.fail_after(3):
        status = await ws.receive_json()
      assert status['status'] == 'busy'
      responses.append({'session': status, 'close': await closed(ws, 4409)})
    assert equal(responses[0], responses[1])
    records.append({'kind': 'foreign-busy', 'path': path, 'responses': responses})
  replacement = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=second&takeover=yes')
  replaces = []
  for pair in (first, first_media):
    responses = [await closed(ws, 4401) for ws in pair]
    assert equal(responses[0], responses[1])
    replaces.append(responses)
  records.append({'kind': 'state-takeover-closes-both-old-channels', 'responses': replaces})
  cavdy = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=cavdy-navdy&takeover=true')
  responses = [await closed(ws, 4401) for ws in replacement]
  assert equal(responses[0], responses[1])
  hud = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=cavdy-navdy&profile=%20CAVDY_HUD%20&map=false')
  for params in owned.params:
    await anyio.to_thread.run_sync(wait_value, params / 'CarrotNaviHudMapProfile', b'1')
    token = await anyio.Path(params / 'CarrotNaviWebBootstrapRequest').read_bytes()
    assert token and int(token, 16) > 0
  values = [payload(await fetch(peer, '/api/carrot_navi/status')) for peer in owned.peers]
  assert equal(values[0], values[1]) and all(row['hudMapProfile'] for row in values)
  records.append({'kind': 'cavdy-profile-even-map-disabled', 'statuses': values, 'token_is_hex': True})
  for params in owned.params:
    await anyio.Path(params / 'ClusterHud').write_bytes(b'1')
  closures = []
  for pair in (cavdy, hud):
    responses = [await closed(ws, 1000) for ws in pair]
    assert equal(responses[0], responses[1])
    closures.append(responses)
  for params in owned.params:
    await anyio.to_thread.run_sync(wait_value, params / 'CarrotNaviHudMapProfile', b'0')
  guard = [await fetch(peer, '/ws/carrot_navi/state?client_id=blocked') for peer in owned.peers]
  assert all(row['status'] == 409 for row in guard) and guard[0]['body_base64'] == guard[1]['body_base64']
  records.append({'kind': 'ClusterHud-guard-active-close', 'closes': closures, 'responses': guard})
  for params in owned.params:
    await anyio.Path(params / 'ClusterHud').write_bytes(b'0')
  reopened = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=after-gate')
  for ws in reopened:
    await ws.close()
  records.append({'kind': 'guard-recovery', 'accepted': True})
  save(output / 'ownership.json', records)


async def quiet_slow(binary: Path, output: Path) -> None:
  async with fixture(binary, output) as owned, AsyncExitStack() as stack:
    publisher = Peer(output / 'publisher')
    await anyio.Path(publisher.output).mkdir()
    channels = []
    try:
      await startup(
        publisher,
        publisher.start(
          [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_publisher.py'))],
          {'services': ['carrotNavi', 'carrotNaviMedia']},
          owned.environment,
          True,
        ),
      )
      for peer in owned.peers:
        stream = await stack.enter_async_context(await anyio.connect_tcp('127.0.0.1', peer.ready['port']))
        reader = await upgrade(stream, '/ws/carrot_navi/media?client_id=quiet-slow&map=0')
        channels.append(reader)
      burst = [media(output, f'quiet-burst-{index}', 'image', 1, b'owned quiet burst', index, name='quiet') for index in range(32)]
      await publisher.control({'frames': burst})
      results = []
      for peer, reader in zip(owned.peers, channels, strict=True):
        binary_frames = 0
        with anyio.fail_after(3):
          while True:
            opcode, raw = await raw_frame(reader)
            if opcode == 8:
              break
            assert opcode == 2
            binary_frames += 1
        eof = False
        with anyio.move_on_after(0.25):
          try:
            await reader.receive()
          except anyio.EndOfStream:
            eof = True
        results.append(
          {
            'provider': peer.output.name,
            'close_code': int.from_bytes(raw[:2], 'big'),
            'close_reason': raw[2:].decode(),
            'eof_without_ack': eof,
            'binary_frames_before_close': binary_frames,
          }
        )
      save(output / 'quiet-slow.json', {'inputs': burst, 'observations': results})
      assert all(row['close_code'] == 1013 and row['close_reason'] == 'carrot_navi_client_slow' and row['eof_without_ack'] for row in results)
    finally:
      await publisher.close()
