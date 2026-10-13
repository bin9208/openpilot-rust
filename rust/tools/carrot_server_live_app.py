#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_live_app.py --binary PATH --output NEW_DIR
# Caller supplies original dependencies and ORIGINAL_PARAMS_BINDING; App wiring and accepted-WebSocket shutdown are observed.
from __future__ import annotations
import argparse
import base64
import json
import os
from pathlib import Path
import shutil
import sys
import time
import uuid

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save
from carrot_server_live_wire import body, binary_packet, fetch, normalize
from carrot_server_live_cases import event


async def run(binary: Path, output: Path) -> None:
  namespace = 'rust-probe-live-app-' + uuid.uuid4().hex
  queue = Path('/dev/shm') / ('msgq_' + namespace)
  queue.mkdir()
  params = output / 'params' / namespace
  params.mkdir(parents=True)
  (params / 'IsMetric').write_bytes(b'1')
  env = {**os.environ, 'OPENPILOT_PREFIX': namespace, 'PARAMS_ROOT': str(output / 'params'), 'CARROT_DATA_DIR': str(output / 'data')}
  peers = [Peer(output / name) for name in ('source', 'native', 'publisher')]
  for peer in peers:
    peer.output.mkdir()
  observations = []
  try:
    publisher = peers[2]
    await startup(
      publisher,
      publisher.start(
        [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_publisher.py'))],
        {'services': ['carState', 'selfdriveState', 'navInstructionCarrot', 'navRoute']},
        env,
        True,
      ),
    )
    for peer, command in zip(peers[:2], ([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_source.py'))], [str(binary)]), strict=True):
      await startup(
        peer,
        peer.start(command, {'params': True, 'unavailable': False, 'composed': True, 'output': str(peer.output), 'state': str(output / 'state')}, env, True),
      )
    first = await fetch(peers[1], '/api/params_bulk?names=OwnedLiveProbe')
    assert first['status'] == 200
    enabled = event(output, 'selfdriveState', {'enabled': True}, 'engaged')
    nav = event(output, 'navInstructionCarrot', {'maneuverPrimaryText': 'owned App navigation'}, 'nav')
    await publisher.control({'frames': [enabled, nav]})
    responses = [await fetch(peer, '/api/live_runtime') for peer in peers[:2]]
    assert [row['status'] for row in responses] == [200, 200]
    assert normalize(body(responses[0])) == normalize(body(responses[1]))
    second = await fetch(peers[1], '/api/params_bulk?names=OwnedLiveProbe')
    assert second['status'] == 200
    engaged = await fetch(peers[0], '/owned/engaged')
    assert body(engaged) == {'engaged': True}
    observations.append({'params_before': first, 'source_native_live': responses, 'params_after': second, 'source_engaged': engaged})
    async with ClientSession() as client:
      sockets = [await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/raw/carState', compress=0) for peer in peers[:2]]
      hello = [await ws.receive_json() for ws in sockets]
      assert hello[0] == hello[1]
      row = event(output, 'carState', {'vEgo': 12.5}, 'raw')
      await publisher.control({'frames': [row]})
      packets = [await binary_packet(ws) for ws in sockets]
      assert packets[0] == packets[1] == await anyio.Path(row['path']).read_bytes()
      for peer in peers[:2]:
        await peer.stop()
      for peer in peers[:2]:
        with anyio.fail_after(3):
          phase = json.loads(await peer.reader.receive_until(b'\n', 65536))
        observations.append({'provider': 'source' if peer is peers[0] else 'native', 'shutdown_phase': phase})
        assert phase == {'stopping': True, 'engaged': True}
        assert peer.process.returncode is None
      await publisher.control({'frames': [row]})
      drained = [await binary_packet(ws) for ws in sockets]
      assert drained[0] == drained[1] == await anyio.Path(row['path']).read_bytes()
      outcomes = []

      async def close(provider: str, ws: ClientWebSocketResponse) -> None:
        start = time.monotonic()
        timed_out = False
        try:
          with anyio.fail_after(3):
            await ws.close(code=3001, message=b'owned grace close')
        except TimeoutError:
          timed_out = True
        outcomes.append({'provider': provider, 'timed_out': timed_out, 'close_code': ws.close_code, 'seconds': time.monotonic() - start})

      async with anyio.create_task_group() as group:
        for provider, ws in zip(('source', 'native'), sockets, strict=True):
          group.start_soon(close, provider, ws)
      outcomes.sort(key=lambda row: row['provider'])
      save(output / 'close-observation.json', outcomes)
      observations.append(
        {
          'ws_hello': hello,
          'before_stop': base64.b64encode(packets[0]).decode(),
          'during_grace': base64.b64encode(drained[0]).decode(),
          'close_outcomes': outcomes,
        }
      )
      assert [(row['timed_out'], row['close_code']) for row in outcomes] == [(True, 1006), (True, 1006)]
  finally:
    failed = sys.exc_info()[0] is not None
    save(output / 'observations.json', observations)
    errors = []
    for peer in peers:
      try:
        await peer.close()
      except (AssertionError, OSError, TimeoutError, anyio.BrokenResourceError, anyio.ClosedResourceError) as error:
        errors.append(f'{type(error).__name__}: {error}')
    save(output / 'cleanup.json', {'errors': errors})
    shutil.rmtree(queue, ignore_errors=True)
    if not failed:
      assert not errors
  save(
    output / 'result.json',
    {'app_live_pair': 1, 'unrelated_params_responses': 2, 'accepted_ws_pairs': 2, 'source_native_close_codes': [1006, 1006], 'owned_peers_exit': 0},
  )


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  save(
    output / 'invocation.json',
    {'argv': [sys.executable, '-P', *sys.argv], 'scope': 'only App/live route isolation and actual graceful stop;21codec+whole-v5 reused'},
  )
  await run(args.binary.resolve(), output)
  print('PASS')


if __name__ == '__main__':
  anyio.run(main)
