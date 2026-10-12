#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_navi.py --binary PATH --output NEW_DIR --close-control
# Caller supplies original dependencies and ORIGINAL_PARAMS_BINDING; owned loopback/IPC only.
from __future__ import annotations

import argparse
from contextlib import AsyncExitStack
import hashlib
import json
import os
from pathlib import Path
import resource
import sys

import anyio
from aiohttp import ClientSession
from carrot_server_dashcam_upload import save
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_navi_http_cases import routes
from carrot_server_navi_ownership import ownership, quiet_slow
from carrot_server_navi_app import application, isolation
from carrot_server_navi_policy import policy
from carrot_server_navi_streams import streams
from carrot_server_navi_fixture import fixture
from carrot_server_navi_wire import acknowledge, fetch, raw_frame, silent_busy, upgrade


async def close_control(binary: Path, output: Path) -> None:
  async with fixture(binary, output) as owned, ClientSession() as client:
    owners = []
    try:
      for peer in owned.peers:
        ws = await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/carrot_navi/state?client_id=owner')
        assert (await ws.receive_json())['status'] == 'accepted'
        owners.append(ws)
      results = {}

      async def observe(provider: str, peer: Peer) -> None:
        results[provider] = await silent_busy(peer)

      async with anyio.create_task_group() as tasks:
        for provider, peer in zip(('source', 'native'), owned.peers, strict=True):
          tasks.start_soon(observe, provider, peer)
      save(output / 'silent-busy.json', results)
      assert results['source']['close_code'] == results['native']['close_code'] == 4409
      assert results['source']['session'] == results['native']['session']
      save(
        output / 'comparison.json',
        {'eof_equal': results['source']['eof'] == results['native']['eof'], 'source_eof': results['source']['eof'], 'native_eof': results['native']['eof']},
      )
    finally:
      for ws in owners:
        await ws.close()


async def whole(binary: Path, output: Path, retained: Path, only_streams: bool) -> None:
  async with fixture(binary, output) as owned, ClientSession() as client:
    publisher = Peer(output / 'publisher')
    await anyio.Path(publisher.output).mkdir()
    sockets = []
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
      if not only_streams:
        await routes(owned, output)
      await streams(owned, publisher, client, sockets, output, retained)
      await ownership(owned, client, sockets, output)
      save(output / 'result.json', {'normal_ipc_media_idle': True, 'ownership': True, 'http_reused': only_streams})
    finally:
      with anyio.CancelScope(shield=True):
        for ws in sockets:
          await ws.close()
        await publisher.close()


async def extra(binary: Path, output: Path, retained: Path) -> None:
  async with fixture(binary, output) as owned, ClientSession() as client:
    publisher = Peer(output / 'publisher')
    await anyio.Path(publisher.output).mkdir()
    sockets = []
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
      await ownership(owned, client, sockets, output)
      await policy(owned, publisher, client, sockets, output, retained)
      save(output / 'result.json', {'ownership_gop_retry_ttl': True})
    finally:
      with anyio.CancelScope(shield=True):
        for ws in sockets:
          await ws.close()
        await publisher.close()


async def takeover_order(binary: Path, output: Path) -> None:
  async with fixture(binary, output) as owned, ClientSession() as client, AsyncExitStack() as stack:
    rows = []
    for peer in owned.peers:
      channels = []
      for mode in ('state', 'media'):
        stream = await stack.enter_async_context(await anyio.connect_tcp('127.0.0.1', peer.ready['port']))
        reader = await upgrade(stream, f'/ws/carrot_navi/{mode}?client_id=old-owner&map=0')
        channels.append((stream, reader))
      ws = await client.ws_connect(f'http://127.0.0.1:{peer.ready["port"]}/ws/carrot_navi/state?client_id=new-owner&takeover=1')
      stack.push_async_callback(ws.close)
      assert (await ws.receive_json())['status'] == 'accepted'
      with anyio.fail_after(3):
        opcode, first = await raw_frame(channels[0][1])
      assert opcode == 8 and int.from_bytes(first[:2], 'big') == 4401
      second = None
      with anyio.move_on_after(0.25):
        opcode, second = await raw_frame(channels[1][1])
        assert opcode == 8
      early_second = second is not None
      eof = False
      with anyio.move_on_after(0.25):
        try:
          await channels[0][1].receive()
        except anyio.EndOfStream:
          eof = True
      rows.append({'provider': peer.output.name, 'first_close': first.hex(), 'second_close_before_first_ack': early_second, 'first_eof_before_ack': eof})
      if not eof:
        await acknowledge(channels[0][0], first)
      if second is None:
        with anyio.fail_after(3):
          opcode, second = await raw_frame(channels[1][1])
        assert opcode == 8
      assert int.from_bytes(second[:2], 'big') == 4401
      try:
        await acknowledge(channels[1][0], second)
      except (anyio.BrokenResourceError, ConnectionResetError):
        assert eof
      await ws.close()
    save(
      output / 'takeover-order.json',
      {
        'observations': rows,
        'order_equal': rows[0]['second_close_before_first_ack'] == rows[1]['second_close_before_first_ack'],
        'eof_equal': rows[0]['first_eof_before_ack'] == rows[1]['first_eof_before_ack'],
      },
    )
    assert all(row['first_eof_before_ack'] for row in rows)


async def invalid_cluster(binary: Path, output: Path) -> None:
  async with fixture(binary, output, cluster=b'owned-invalid-integer', expected=-6) as owned:
    rows = []
    for peer in owned.peers:
      outcome = 'unexpected response'
      try:
        response = await fetch(peer, '/api/carrot_navi/status')
        outcome = str(response['status'])
      except (anyio.EndOfStream, anyio.IncompleteRead, anyio.BrokenResourceError, ConnectionResetError):
        outcome = 'EOF'
      with anyio.fail_after(3):
        await peer.process.wait()
      rows.append({'provider': peer.output.name, 'http': outcome, 'exit': peer.process.returncode})
    save(output / 'invalid-cluster.json', {'core_limit': resource.getrlimit(resource.RLIMIT_CORE), 'observations': rows})
    assert all(row['http'] == 'EOF' and row['exit'] == -6 for row in rows)


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--close-control', action='store_true')
  parser.add_argument('--invalid-cluster', action='store_true')
  parser.add_argument('--whole', action='store_true')
  parser.add_argument('--streams-only', action='store_true')
  parser.add_argument('--extra', action='store_true')
  parser.add_argument('--takeover-order', action='store_true')
  parser.add_argument('--quiet-slow', action='store_true')
  parser.add_argument('--app', action='store_true')
  parser.add_argument('--app-isolation', action='store_true')
  parser.add_argument('--retained-mux', type=Path)
  args = parser.parse_args()
  binary = args.binary.resolve()
  output = args.output.resolve()
  await anyio.Path(output).mkdir(parents=True)
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  save(
    output / 'invocation.json',
    {
      'argv': [sys.executable, '-P', *sys.argv],
      'PYTHONPATH': os.environ.get('PYTHONPATH', ''),
      'binary': str(binary),
      'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    },
  )
  if args.close_control:
    await close_control(binary, output / 'close-control')
  if args.app:
    await application(binary, output / 'application')
  if args.app_isolation:
    await isolation(binary, output / 'app-isolation')
  if args.quiet_slow:
    await quiet_slow(binary, output / 'quiet-slow')
  if args.takeover_order:
    await takeover_order(binary, output / 'takeover-order')
  if args.invalid_cluster:
    await invalid_cluster(binary, output / 'invalid-cluster')
  if args.whole:
    assert args.retained_mux is not None
    await whole(binary, output / 'whole', args.retained_mux.resolve(), args.streams_only)
  if args.extra:
    assert args.retained_mux is not None
    await extra(binary, output / 'extra', args.retained_mux.resolve())
  print(json.dumps({'output': str(output), 'finished': True}))


if __name__ == '__main__':
  anyio.run(main)
