#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_settings_snapshot_refresh.py BINARY NEW_OUTPUT
from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import time

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_sync_probe import Json, Peer, startup
from carrot_server_settings_snapshot import BINDING, ROOT, SOURCE, comparable, fetch, populate, setup


class Receiver:
  def __init__(self) -> None:
    self.gets = 0
    self.posts = 0
    self.first = anyio.Event()
    self.second = anyio.Event()
    self.third = anyio.Event()
    self.release = anyio.Event()

  async def handle(self, stream: anyio.abc.SocketStream) -> None:
    async with stream:
      reader = BufferedByteReceiveStream(stream)
      head = await reader.receive_until(b'\r\n\r\n', 16384)
      if head.startswith(b'POST '):
        self.posts += 1
        length = next((int(line.split(b':', 1)[1]) for line in head.split(b'\r\n') if line.lower().startswith(b'content-length:')), 0)
        await reader.receive_exactly(length)
        await stream.send(b'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}')
      else:
        self.gets += 1
        self.first.set()
        if self.gets >= 2:
          self.second.set()
        if self.gets >= 3:
          self.third.set()
        await self.release.wait()


async def scenario(binary: Path, output: Path) -> None:
  environment = json.loads((ROOT / '.omo/evidence/carrot-server-225-resume/live-runtime/application-ruff-v4-invocation.json').read_text())
  peers = [Peer(output / name) for name in ['source', 'native']]
  receivers = [Receiver(), Receiver()]
  observations: list[Json] = []
  try:
    async with anyio.create_task_group() as tasks:
      for peer, receiver, command in zip(peers, receivers, [[environment['argv'][0], '-P', str(SOURCE)], [str(binary)]], strict=True):
        root = peer.output
        await anyio.to_thread.run_sync(setup, root)
        await anyio.Path(root / 'params/d').mkdir(parents=True)
        await anyio.to_thread.run_sync(populate, root)
        listener = await anyio.create_tcp_listener(local_host='127.0.0.1', local_port=0)
        tasks.start_soon(listener.serve, receiver.handle)
        port = listener.extra(anyio.abc.SocketAttribute.local_address)[1]
        config = {
          'root': str(root),
          'source': str(ROOT),
          'binding': str(BINDING),
          'params': True,
          'refresh': True,
          'popular': {'ok': True, 'car_key': 'owned-car', 'popular_values': {'FutureSetting': 11}},
        }
        env = os.environ | {
          'PYTHONPATH': environment['PYTHONPATH'],
          'PARAMS_ROOT': str(root / 'params'),
          'OPENPILOT_PREFIX': 'd',
          'CARROT_DATA_DIR': str(root),
          'CARROT_SETTINGS_PATH': str(root / 'settings.json'),
          'CARROT_PARAM_VALUE_SNAPSHOT_URL': f'http://127.0.0.1:{port}/snapshot',
          'CARROT_PARAM_VALUE_POPULAR_URL': f'http://127.0.0.1:{port}/popular',
          'CARROT_PARAM_VALUE_CF_ID': 'owned-id',
          'CARROT_PARAM_VALUE_CF_SECRET': 'owned-secret',
        }
        await startup(peer, peer.start(command, config, env, True))
        with anyio.fail_after(2):
          await receiver.first.wait()
      for peer, receiver in zip(peers, receivers, strict=True):
        started = time.monotonic()
        response = await fetch(peer)
        assert time.monotonic() - started < 1
        with anyio.fail_after(2):
          await receiver.second.wait()
        second = await fetch(peer)
        with anyio.move_on_after(0.25) as quiet:
          await receiver.third.wait()
        assert quiet.cancel_called and receiver.gets == 2 and receiver.posts == 1
        assert comparable(response, peer.output) == comparable(second, peer.output)
        observations.append({'response': response, 'second': second, 'gets_held': receiver.gets, 'posts': receiver.posts, 'responded_while_held': True})
      assert comparable(observations[0]['response'], peers[0].output) == comparable(observations[1]['response'], peers[1].output)
      for peer in peers:
        await peer.stop()
        with anyio.fail_after(5):
          await peer.close()
      tasks.cancel_scope.cancel()
    cleanup = [json.loads(await anyio.Path(peer.output / 'cleanup.json').read_text()) for peer in peers]
    assert all(row['exit'] == 0 and row['errors'] == [] for row in cleanup)
    await anyio.Path(output / 'result.json').write_text(json.dumps({'equal': True, 'observations': observations, 'cleanup': cleanup}, indent=2) + '\n')
  finally:
    for peer in peers:
      if peer.process is not None and peer.process.returncode is None:
        with anyio.CancelScope(shield=True):
          await peer.close()


if __name__ == '__main__':
  anyio.run(scenario, Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
