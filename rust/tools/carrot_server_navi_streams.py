# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Actual IPC -> WebSocket path; fMP4 provider-container variance is explicitly compared through mdat.
from __future__ import annotations

import base64
from pathlib import Path

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse
from carrot_server_dashcam_sync_probe import Peer
from carrot_server_dashcam_upload import save
from carrot_server_live_wire import binary_packet
from carrot_server_navi_cases import map_frames, media, state
from carrot_server_navi_fixture import Fixture
from carrot_server_navi_http_cases import equal
from carrot_server_navi_mux import source_module
from carrot_server_navi_wire import Packet, fetch, packet, payload


async def connect(owned: Fixture, client: ClientSession, sockets: list[ClientWebSocketResponse], path: str) -> list[ClientWebSocketResponse]:
  result = []
  for peer in owned.peers:
    ws = await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}' + path, compress=0)
    sockets.append(ws)
    with anyio.fail_after(3):
      accepted = await ws.receive_json()
    assert accepted['status'] == 'accepted', accepted
    result.append(ws)
  return result


async def pair_packets(sockets: list[ClientWebSocketResponse], remux: bool = False) -> list[Packet]:
  raw = [await binary_packet(ws) for ws in sockets]
  rows = [packet(value) for value in raw]
  assert equal(rows[0]['metadata'], rows[1]['metadata']), rows
  if remux and rows[0]['metadata']['type'] == 'carrotNaviFmp4':
    boxes = source_module()._boxes
    mdats = [[bytes(part) for name, part in boxes(base64.b64decode(row['payload'])) if name == 'mdat'] for row in rows]
    assert mdats[0] == mdats[1]
    assert bool(mdats[0]) == (rows[0]['metadata']['messageType'] == 3)
    assert bool(base64.b64decode(rows[0]['payload'])) == bool(base64.b64decode(rows[1]['payload']))
  else:
    assert raw[0] == raw[1]
  return rows


async def streams(owned: Fixture, publisher: Peer, client: ClientSession, sockets: list[ClientWebSocketResponse], output: Path, retained: Path) -> None:
  records = []
  states = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=normal')
  event = state(output)
  await publisher.control({'frames': [event]})
  with anyio.fail_after(3):
    values = [await ws.receive_json() for ws in states]
  records.append({'kind': 'actual-carrotNavi', 'values': values})
  save(output / 'stream-observations.json', records)
  assert equal(values[0], values[1]), values
  assert values[0]['state']['vehicle']['roadName'] == 'owned 한글 road'
  media_ws = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=normal')
  image = media(output, 'image', 'web_image', 1, b'owned-image-payload', name='overlay-a')
  await publisher.control({'frames': [image]})
  records.append({'kind': 'actual-carrotNaviMedia-image', 'packets': await pair_packets(media_ws)})
  header, frames = map_frames(output, retained)
  await publisher.control({'frames': [header]})
  for index, frame in enumerate(frames):
    await publisher.control({'frames': [frame]})
    records.append({'kind': 'actual-carrotNaviMedia-fmp4', 'sample': index, 'packets': await pair_packets(media_ws, True)})
    save(output / 'stream-observations.json', records)
  responses = [await fetch(peer, '/api/carrot_navi/status') for peer in owned.peers]
  values = [payload(row) for row in responses]
  records.append({'kind': 'stream-status', 'responses': responses})
  save(output / 'stream-observations.json', records)
  assert equal(values[0], values[1]), values
  assert values[0]['stateMessages'] == 1 and values[0]['mediaMessages'] == 10
  assert values[0]['mapStream']['frames'] == 8 and values[0]['mapStream']['webPipeline']['active']
  cached_state = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=normal')
  with anyio.fail_after(3):
    cached = [await ws.receive_json() for ws in cached_state]
  assert equal(cached[0], cached[1])
  records.append({'kind': 'fresh-state-bootstrap', 'values': cached})
  cached_media = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=normal')
  count = values[0]['mapStream']['gopFrames'] + 2
  for _ in range(count):
    records.append({'kind': 'fresh-map-image-bootstrap', 'packets': await pair_packets(cached_media, True)})
  no_map = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=normal&map=0')
  records.append({'kind': 'map-disabled-image-bootstrap', 'packets': await pair_packets(no_map)})
  for ws in media_ws + cached_media:
    await ws.close()
  image2 = media(output, 'image2', 'image', 1, b'owned-image-after-demand-loss', sequence=2, name='overlay-a')
  await publisher.control({'frames': [image2]})
  records.append({'kind': 'image-after-map-demand-loss', 'packets': await pair_packets(no_map)})
  responses = [await fetch(peer, '/api/carrot_navi/status') for peer in owned.peers]
  values = [payload(row) for row in responses]
  assert equal(values[0], values[1]), values
  assert not values[0]['mapStream']['webPipeline']['active'] and values[0]['mapStream']['gopFrames'] == 0
  records.append({'kind': 'map-demand-loss', 'responses': responses})
  for ws in states + cached_state + no_map:
    await ws.close()
  await anyio.sleep(3.1)
  idle = []
  for peer in owned.peers:
    maps = await anyio.Path(f'/proc/{peer.process.pid}/maps').read_text()
    names = [name for name in ('carrotNavi', 'carrotNaviMedia') if f'/dev/shm/msgq_{owned.namespace}/{name}' in maps]
    response = await fetch(peer, '/api/carrot_navi/status')
    assert not names and not payload(response)['stateFresh'] and not payload(response)['readerActive']
    idle.append({'provider': peer.output.name, 'mappings': names, 'response': response})
  records.append({'kind': 'idle-sockets-closed-stale-cache-retained', 'observations': idle})
  reopened = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=normal')
  await publisher.control({'frames': [event]})
  with anyio.fail_after(3):
    reopened_values = [await ws.receive_json() for ws in reopened]
  assert equal(reopened_values[0], reopened_values[1])
  records.append({'kind': 'after-idle-first-state-no-cached-stale-packet', 'values': reopened_values})
  for ws in reopened:
    await ws.close()
  save(output / 'stream-observations.json', records)
