from __future__ import annotations

import asyncio
import json
from pathlib import Path
import struct

import aiohttp

from carrot_navi_cases import query
from carrot_navi_qa.cereal import Observer
from carrot_navi_qa.socket import Client
from card_runtime_source import load_binding


async def boundaries(client: Client, observer: Observer, params_root: Path, binding: Path) -> None:
  load_binding(binding)
  from openpilot.common.params import Params
  params = Params(str(params_root))
  initial = await client.http_read()
  await observer.observe(initial)
  baseline = len(observer.publications)
  deadline = asyncio.get_running_loop().time() + 3
  while len(observer.publications) < baseline + 3:
    observer.drain()
    if asyncio.get_running_loop().time() >= deadline:
      raise TimeoutError('idle heartbeat publications absent')
    await asyncio.sleep(.001)
  idle = [row['event']['carrotNavi'] for row in observer.publications[baseline:]]
  gaps = [(right['publishMonoTimeNanos'] - left['publishMonoTimeNanos']) / 1e9
    for left, right in zip(idle, idle[1:], strict=False)]
  assert all(body['generation'] == initial['state_generation'] for body in idle)
  assert all(.48 <= gap <= .8 for gap in gaps), gaps
  client.rows.append({'action': 'idle heartbeat', 'value': {'unchanged_generation': True, 'valid': True}})

  async with client.http.ws_connect(client.url + '/api/navi/ws/v2/control/lifecycle') as control:
    await control.send_json(query())
    manifest = await client.response(control, 'manifest')
    session = manifest['session_id']
    streams = {(row['kind'], row['name']): row['stream_handle'] for row in manifest['streams']}
    async with client.http.ws_connect(client.url + f'/api/navi/ws/v2/json/{session}/vehicle') as vehicle:
      observer.drain()
      before = len(observer.publications)
      for sequence in range(1, 13):
        await vehicle.send_json({'type': 'item_update', 'protocol_version': 2, 'session_id': session,
          'kind': 'json', 'name': 'vehicle', 'schema_version': 1, 'stream_handle': streams['json', 'vehicle'],
          'manifest_revision': 1, 'sequence': sequence, 'source_timestamp_ms': 1000 + sequence,
          'sent_at_ms': 2000 + sequence, 'present': True, 'value': {'speed': sequence, 'foreground': True}})
      health = await client.health_until('received_count', 12)
      state_updates = [row['event']['carrotNavi'] for row in observer.publications[before:]
        if row['topic'] == 'carrotNavi']
      assert 1 <= len(state_updates) <= 3, len(state_updates)
      assert state_updates[-1]['vehicle']['meta']['sequence'] == 12
      client.rows.append({'action': 'burst coalescing', 'value': {'final_sequence': 12, 'bounded': True}})

      async with client.http.ws_connect(client.url + f'/api/navi/ws/v2/image/{session}/tbt_next') as image, \
        client.http.ws_connect(client.url + f'/api/navi/ws/v2/render/{session}/map_main') as render:
        payload = b'\x89PNG\r\n\x1a\nowned-bootstrap'
        observer.register_media('image:tbt_next', payload)
        await image.send_bytes(struct.pack('>4sBBBBIIQQIHH', b'CNV2', 2, 1, 1, 0,
          streams['image', 'tbt_next'], 1, 1, 1001, len(payload), 32, 24) + payload)
        health = await client.health_until('received_count', 13)
        render_payloads = (b'\x00\x00\x00\x01\x67owned-config', b'\x00\x00\x00\x01\x65owned-keyframe')
        for sequence, render_payload in enumerate(render_payloads, start=1):
          observer.register_media('render:map_main', render_payload)
          await render.send_bytes(struct.pack('>4sBBBBIIQQIHH', b'CNV2', 2, sequence + 1, 3,
            sequence - 1, streams['render', 'map_main'], 1, sequence, 1001, len(render_payload), 960, 540) + render_payload)
          health = await client.health_until('received_count', 13 + sequence)
        for request in ('owned-1', 'owned-2'):
          observer.drain()
          before = len(observer.publications)
          params.put('CarrotNaviWebBootstrapRequest', request)
          deadline = asyncio.get_running_loop().time() + 2
          while True:
            observer.drain()
            bootstrap = [row['event']['carrotNaviMedia'] for row in observer.publications[before:]
              if row['topic'] == 'carrotNaviMedia' and row['event']['carrotNaviMedia']['kind'] in ('web_image', 'web_render')]
            if len(bootstrap) == 3:
              assert [body['payload'] for body in bootstrap] == [*render_payloads, payload]
              assert [body['sequence'] for body in bootstrap] == [1, 2, 1]
              assert all(body['sessionId'] == session for body in bootstrap)
              break
            if asyncio.get_running_loop().time() >= deadline:
              raise TimeoutError('Params media bootstrap absent')
            await asyncio.sleep(.001)
        client.rows.append({'action': 'Params bootstrap repeated request change', 'value': {'copies': 6, 'payload_equal': True}})
        observer.register_media('image:tbt_next', b'')
        await image.send_bytes(struct.pack('>4sBBBBIIQQIHH', b'CNV2', 2, 4, 2, 0,
          streams['image', 'tbt_next'], 1, 2, 1002, 0, 0, 0))
        health = await client.health_until('received_count', 16)
        assert health['items']['image:tbt_next']['present'] is False
        client.rows.append({'action': 'media clear publication', 'value': {'sequence': 2, 'present': False, 'empty_payload': True}})

        for key, value in (('ClusterNaviMapTheme', 2), ('ClusterNaviMapType', 1),
          ('ClusterNaviMapFps', 3), ('CarrotNaviHudMapProfile', 1)):
          params.put(key, value)
        for socket in (control, vehicle, image, render):
          message = await socket.receive(timeout=3)
          assert message.type == aiohttp.WSMsgType.CLOSE and message.data == 1012 and message.extra == 'map configuration changed', message
        await client.health_until('control_connections', 0)

  async with client.http.ws_connect(client.url + '/api/navi/ws/v2/control/reconnected') as control:
    await control.send_json(query())
    new_manifest = await client.response(control, 'reconnected manifest')
    assert new_manifest['session_id'] != session
    health = await client.health_until('session_received_count', 0)
    assert (health['map_theme'], health['map_type'], health['map_hz'], health['map_bitrate_kbps'],
      health['screen_center_y_ratio']) == ('light', 'satellite', 30, 6000, .68), health
    assert health['items'] == {}
    await client.rejected_stream(f'/api/navi/ws/v2/json/{session}/vehicle')
    generation = health['state_generation']
    params.put('ClusterNaviMapFps', 3)
    try:
      message = await control.receive(timeout=1.2)
    except TimeoutError:
      pass
    else:
      raise AssertionError(f'unchanged map unexpectedly closed: {message}')
    health = await client.http_read()
    assert health['state_generation'] == generation
    client.rows.append({'action': 'unchanged Params preserve session', 'value': {'generation_unchanged': True}})
  await client.health_until('control_connections', 0)

  async with client.http.ws_connect(client.url + '/api/navi/ws/v2/control/heartbeat', autoping=False) as control:
    message = await control.receive(timeout=6)
    assert message.type == aiohttp.WSMsgType.PING and message.data == b''
    await control.pong(message.data)
    await control.send_json(query())
    await client.response(control, 'WebSocket pong recovery manifest')
  await client.health_until('control_connections', 0)
  observer.output.joinpath('heartbeat.json').write_text(json.dumps({'idle_publish_gaps_seconds': gaps,
    'coalesced_state_messages': len(state_updates), 'websocket_ping_pong': True}, indent=2))
