# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Actual Application composition and source AppRunner cleanup; unchanged standalone packet corpus is reused.
from __future__ import annotations

import json
from pathlib import Path
import sys
import time

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save
from carrot_server_live_wire import binary_packet
from carrot_server_navi_cases import media, state
from carrot_server_navi_fixture import fixture
from carrot_server_navi_http_cases import equal
from carrot_server_navi_ownership import wait_value
from carrot_server_navi_streams import connect
from carrot_server_navi_wire import fetch, payload


async def application(binary: Path, output: Path) -> None:
  async with fixture(binary, output, composed=True) as owned, ClientSession() as client:
    publisher = Peer(output / 'publisher')
    await anyio.Path(publisher.output).mkdir()
    sockets = []
    records = []
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
      unrelated = await fetch(owned.peers[1], '/api/params_bulk?names=OwnedNaviProbe')
      assert unrelated['status'] == 200
      caps = [await fetch(peer, '/api/carrot_navi/capabilities') for peer in owned.peers]
      assert equal(payload(caps[0]), payload(caps[1]))
      head = [await fetch(peer, '/api/carrot_navi/status', 'HEAD') for peer in owned.peers]
      assert all(row['status'] == 200 and not row['body_base64'] for row in head)
      for peer in owned.peers:
        assert (await fetch(peer, '/api/carrot_navi/client_diagnostic', 'POST', b'{"phase":"application"}', '127.0.0.2'))['status'] == 200
      diagnostic = [await fetch(peer, '/api/carrot_navi/status') for peer in owned.peers]
      assert equal(payload(diagnostic[0]), payload(diagnostic[1]))
      assert payload(diagnostic[1])['clientDiagnostics'][0]['peer'] == '127.0.0.2'
      records.append({'unrelated': unrelated, 'capabilities': caps, 'head': head, 'actual_peer_diagnostic': diagnostic})
      states = await connect(owned, client, sockets, '/ws/carrot_navi/state?client_id=cavdy-navdy')
      images = await connect(owned, client, sockets, '/ws/carrot_navi/media?client_id=cavdy-navdy&profile=cavdy_hud&map=0')
      for params in owned.params:
        await anyio.to_thread.run_sync(wait_value, params / 'CarrotNaviHudMapProfile', b'1')
      event = state(output)
      image = media(output, 'application-image', 'image', 1, b'owned App image', name='overlay')
      await publisher.control({'frames': [event, image]})
      with anyio.fail_after(3):
        before = [await ws.receive_json() for ws in states]
      wires = [await binary_packet(ws) for ws in images]
      assert equal(before[0], before[1]) and wires[0] == wires[1]
      for peer in owned.peers:
        await peer.stop()
        with anyio.fail_after(3):
          phase = json.loads(await peer.reader.receive_until(b'\n', 65536))
        assert phase == {'stopping': True} and peer.process.returncode is None
      await publisher.control({'frames': [event, image]})
      with anyio.fail_after(3):
        during = [await ws.receive_json() for ws in states]
      drained = [await binary_packet(ws) for ws in images]
      assert equal(during[0], during[1]) and drained[0] == drained[1] == wires[0]
      outcomes = []

      async def close(provider: str, mode: str, ws: ClientWebSocketResponse) -> None:
        started = time.monotonic()
        timed_out = False
        try:
          with anyio.fail_after(3):
            await ws.close(code=3001, message=b'owned App grace close')
        except TimeoutError:
          timed_out = True
        outcomes.append({'provider': provider, 'mode': mode, 'timed_out': timed_out, 'close_code': ws.close_code, 'seconds': time.monotonic() - started})

      async with anyio.create_task_group() as group:
        for mode, pair in [('state', states), ('media', images)]:
          for provider, ws in zip(('source', 'native'), pair, strict=True):
            group.start_soon(close, provider, mode, ws)
      outcomes.sort(key=lambda row: (row['mode'], row['provider']))
      records.append({'before_stop': before, 'during_grace': during, 'close_outcomes': outcomes})
      save(output / 'close-observation.json', outcomes)
      assert all(row['timed_out'] and row['close_code'] == 1006 for row in outcomes)
      for peer in owned.peers:
        with anyio.fail_after(5):
          await peer.process.wait()
        assert peer.process.returncode == 0
      for params in owned.params:
        await anyio.to_thread.run_sync(wait_value, params / 'CarrotNaviHudMapProfile', b'0')
      save(
        output / 'result.json',
        {
          'app_routes': True,
          'actual_remote': '127.0.0.2',
          'packets_during_shared_grace': True,
          'owned_process_exits': [peer.process.returncode for peer in owned.peers],
          'profile_reset': [0, 0],
        },
      )
    finally:
      save(output / 'observations.json', records)
      with anyio.CancelScope(shield=True):
        for ws in sockets:
          await ws.close()
        await publisher.close()


async def isolation(binary: Path, output: Path) -> None:
  for name, params, unavailable in [('unavailable', True, True), ('no-params', False, False)]:
    async with fixture(binary, output / name, composed=True, params=params, unavailable=unavailable) as owned:
      rows = []
      for path, method, body in [
        ('/api/carrot_navi/capabilities', 'GET', b''),
        ('/api/carrot_navi/status', 'GET', b''),
        ('/api/carrot_navi/client_diagnostic', 'POST', b'{}'),
        ('/ws/carrot_navi/state', 'GET', b''),
      ]:
        responses = [await fetch(peer, path, method, body) for peer in owned.peers]
        assert responses[0]['status'] == responses[1]['status']
        if responses[0]['status'] == 200:
          assert equal(payload(responses[0]), payload(responses[1]))
        else:
          assert responses[0]['body_base64'] == responses[1]['body_base64']
        rows.append({'path': path, 'responses': responses})
      expected = [200, 503, 503, 503] if unavailable else [200, 200, 200, 409]
      assert [row['responses'][0]['status'] for row in rows] == expected
      unrelated = await fetch(owned.peers[1], '/api/params_bulk?names=OwnedNaviProbe')
      assert unrelated['status'] == 200
      save(output / name / 'isolation.json', {'responses': rows, 'unrelated_native_route': unrelated})
