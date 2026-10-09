# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Bounded GOP/queue/retry/TTL controls using retained valid H264 and owned malformed configuration.
from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path
import time

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse
from carrot_server_dashcam_sync_probe import Peer
from carrot_server_dashcam_upload import save
from carrot_server_navi_cases import map_frames, media
from carrot_server_navi_fixture import Fixture
from carrot_server_navi_http_cases import equal
from carrot_server_navi_ownership import closed
from carrot_server_navi_streams import connect, pair_packets
from carrot_server_navi_wire import fetch, payload


async def policy(owned: Fixture, publisher: Peer, client: ClientSession, sockets: list[ClientWebSocketResponse], output: Path, retained: Path) -> None:
  started = time.monotonic()
  for peer in owned.peers:
    assert (await fetch(peer, '/api/carrot_navi/client_diagnostic', 'POST', b'{"phase":"policy"}'))['status'] == 200
  channels = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=policy')
  header, samples = map_frames(output, retained)
  await publisher.control({'frames': [header, samples[0]]})
  initialization = await pair_packets(channels, True)
  raw = await anyio.Path(samples[1]['path']).read_bytes()
  inputs = []
  hashes = []
  for sequence in range(2, 98):
    row = media(output, f'long-gop-{sequence}', 'render', 3, raw, sequence, 1000 + sequence * 200)
    inputs.append(row)
    await publisher.control({'frames': [row]})
    pair = await pair_packets(channels, True)
    hashes.append([hashlib.sha256(base64.b64decode(item['payload'])).hexdigest() for item in pair])
  statuses = [payload(await fetch(peer, '/api/carrot_navi/status')) for peer in owned.peers]
  assert equal(statuses[0], statuses[1]) and all(row['mapStream']['gopFrames'] == 90 for row in statuses)
  bootstrap = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=policy')
  cached = [await pair_packets(bootstrap, True) for _ in range(12)]
  assert cached[0][0]['metadata']['messageType'] == 2
  save(
    output / 'gop-cap.json',
    {
      'initialization': initialization,
      'inputs': inputs,
      'output_hashes': hashes,
      'statuses': statuses,
      'bootstrap_first12': cached,
      'gop_cap': 90,
      'client_bootstrap_cap': 12,
    },
  )
  for ws in bootstrap:
    await ws.close()
  burst = [media(output, f'burst-{index}', 'image', 1, b'owned-burst', index, name='burst') for index in range(32)]
  await publisher.control({'frames': burst})
  closures = [await closed(ws, 1013) for ws in channels]
  assert equal(closures[0], closures[1])
  save(output / 'slow-queue.json', {'inputs': burst, 'closes': closures, 'queue_limit': 12})
  retry = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=retry')
  retained_image = await pair_packets(retry)
  assert all(row['metadata']['kind'] == 'image' and row['metadata']['name'] == 'burst' for row in retained_image)
  save(output / 'retry-bootstrap.json', {'retained_image_after_map_demand_loss': retained_image})
  bad = media(output, 'bad-config', 'render', 2, b'owned-invalid-h264', session='owned-retry')
  key = media(output, 'bad-keyframe', 'render', 3, await anyio.Path(samples[0]['path']).read_bytes(), keyframe=True, session='owned-retry')
  await publisher.control({'frames': [bad, key]})
  failure = await pair_packets(retry)
  save(output / 'retry-first-failure.json', failure)
  assert all(row['metadata']['reason'] == 'server_remux_error' and not row['payload'] for row in failure)
  before = time.monotonic()
  await publisher.control({'frames': [key]})
  for ws in retry:
    with anyio.move_on_after(0.15) as scope:
      message = await ws.receive()
      raise AssertionError(f'premature remux retry output: {message}')
    assert scope.cancel_called
  await anyio.sleep(max(0, 5.1 - (time.monotonic() - before)))
  nonkey = media(output, 'retry-nonkey', 'render', 3, raw, session='owned-retry')
  await publisher.control({'frames': [nonkey]})
  for ws in retry:
    with anyio.move_on_after(0.15) as scope:
      message = await ws.receive()
      raise AssertionError(f'nonkey retry output: {message}')
    assert scope.cancel_called
  await publisher.control({'frames': [key]})
  after_deadline = await pair_packets(retry)
  assert all(row['metadata']['reason'] == 'server_remux_error' for row in after_deadline)
  config_payload = json.loads(await anyio.Path(retained / 'input.json').read_text())['config']
  valid = media(output, 'valid-recovery-config', 'render', 2, await anyio.Path(config_payload).read_bytes(), session='owned-retry')
  await publisher.control({'frames': [valid, key]})
  recovery = await pair_packets(retry, True)
  statuses = [payload(await fetch(peer, '/api/carrot_navi/status')) for peer in owned.peers]
  assert equal(statuses[0], statuses[1]) and all(row['mapStream']['webPipeline']['active'] for row in statuses)
  save(
    output / 'retry.json',
    {
      'invalid_config': bad,
      'keyframe': key,
      'failure': failure,
      'early_retry_suppressed': True,
      'nonkey_after_deadline_suppressed': True,
      'after_deadline': after_deadline,
      'recovery': recovery,
      'statuses': statuses,
    },
  )
  for ws in retry:
    await ws.close()
  await anyio.sleep(max(0, 15.1 - (time.monotonic() - started)))
  responses = [await fetch(peer, '/api/carrot_navi/status') for peer in owned.peers]
  assert all(payload(row)['clientDiagnostics'] == [] for row in responses)
  save(output / 'diagnostic-ttl.json', {'seconds': time.monotonic() - started, 'responses': responses})
