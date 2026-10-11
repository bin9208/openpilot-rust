#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import json
from pathlib import Path
import time

import anyio
from aiohttp import ClientSession, ClientConnectorError
from carrot_server_support_fixture import Fixture
from carrot_server_terminal_cli_fixture import Inputs
from carrot_server_terminal_cleanup import identity


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  await anyio.Path(root / 'hold-term').write_text('owned')
  fixture = Fixture(root, Inputs(args.command, args.launcher, args.binding, args.vision_root), args.server, False, True)
  try:
    await fixture.start()
    async with ClientSession() as client:
      async with client.ws_connect(fixture.url + '/ws/terminal_pty') as local:
        terminal = await local.receive_json()
      async with client.post(fixture.url + '/api/support_terminal/start', json={}) as response:
        session = await response.json()
        assert response.status == 200 and session['state'] == 'sharing'
      child = json.loads(await anyio.Path(root / 'cloud-child.json').read_text())
      async with client.post(fixture.url + '/api/support_terminal/stop') as response:
        assert response.status == 200 and not (await response.json())['active']
      with anyio.fail_after(2):
        while not await anyio.Path(root / 'cloud-term').exists():
          await anyio.sleep(0.01)
      assert await anyio.to_thread.run_sync(identity, child['pid']) is not None
      began = time.monotonic()
      peer = fixture.peer
      assert peer.process and peer.process.stdin and peer.reader
      await peer.process.stdin.send(b'force\n')
      with anyio.fail_after(2):
        cleanup = json.loads(await peer.reader.receive_until(b'\n', 65536))
        await peer.process.wait()
      elapsed = time.monotonic() - began
      assert cleanup['app_cleanup'] and elapsed < 2
      assert await anyio.to_thread.run_sync(identity, child['pid']) is None
      assert await anyio.to_thread.run_sync(identity, terminal['pid']) is None
      reachable = True
      try:
        async with client.get(session['local_url']) as response:
          reachable = response.status == 200
      except ClientConnectorError:
        reachable = False
      assert not reachable
      await fixture.record('force-result.json', {'seconds': elapsed, 'held_term_seen': True, 'tunnel_reaped': True, 'pty_reaped': True, 'guest_listener_closed': True, 'app_cleanup': cleanup, 'exit': peer.process.returncode})
  finally:
    await fixture.close()


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'command', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)
