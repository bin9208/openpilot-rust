#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import gzip
import json
import resource
from pathlib import Path
import time

import anyio
from aiohttp import ClientSession, web, ServerDisconnectedError
from carrot_server_support_fixture import Fixture
from carrot_server_terminal_cli_fixture import Inputs


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  received = []

  async def upstream(request: web.Request) -> web.Response:
    body = await request.read()
    received.append({'bytes': list(body), 'content_type': request.headers.get('Content-Type')})
    await anyio.to_thread.run_sync((root / 'upstream.json').write_text, json.dumps(received))
    if body == b'timeout':
      await anyio.sleep(6)
    if body == b'gzip':
      return web.Response(body=gzip.compress(b'decoded\x00\xff'), headers={'Content-Encoding': 'gzip', 'Content-Type': 'application/x-reply'})
    return web.Response(body=b'upstream\x00\xff' + body, status=418 if body == b'http-error' else 201, headers={'Content-Type': 'application/x-reply; version=1'})

  receiver = web.Application()
  receiver.router.add_post('/stream', upstream)
  runner = web.AppRunner(receiver)
  await runner.setup()
  listener = await anyio.create_tcp_listener(local_host='127.0.0.1', local_port=0)
  port = listener.extra(anyio.abc.SocketAttribute.local_address)[1]
  await listener.aclose()
  await web.TCPSite(runner, '127.0.0.1', port).start()
  fixture = Fixture(root, Inputs(args.command, args.launcher, args.binding, args.vision_root), args.server, args.provider == 'source', True)
  setup_original = fixture.peer.fixture.setup

  async def setup() -> None:
    await setup_original()
    await anyio.Path(root / 'stream-target.json').write_text(json.dumps({'target': f'http://127.0.0.1:{port}/stream'}))

  fixture.peer.fixture.setup = setup
  observations = []
  try:
    await fixture.start()
    async with ClientSession() as client:
      if args.fatal:
        params = fixture.peer.fixture.params / fixture.peer.fixture.prefix
        await anyio.Path(params / 'ClusterHud').write_bytes(args.fatal.encode())
        disconnected = False
        try:
          async with client.post(fixture.url + '/stream', data=b'fatal') as response:
            await fixture.record('fatal-response.json', {'status': response.status, 'body': list(await response.read())})
        except ServerDisconnectedError:
          disconnected = True
        process = fixture.peer.process
        assert process
        if disconnected:
          with anyio.fail_after(3):
            await process.wait()
        await fixture.record('fatal-result.json', {'disconnected': disconnected, 'exit': process.returncode, 'input': args.fatal})
        assert disconnected and process.returncode == -6
        return
      for body in [b'\xff\x00raw\x80', b'http-error', b'gzip', b'timeout']:
        began = time.monotonic()
        async with client.post(fixture.url + '/stream', data=body, headers={'Content-Type': 'application/x-owned; charset=iso-8859-1'}) as response:
          reply = await response.read()
          elapsed = time.monotonic() - began
          observations.append({'status': response.status, 'request': list(body), 'reply': list(reply), 'content_type': response.headers.get('Content-Type'), 'seconds': elapsed})
          await fixture.record('stream-result.json', observations)
          if body == b'timeout':
            assert response.status == 504 and json.loads(reply)['error'] == 'webrtcd timeout' and 4.8 <= elapsed < 6.5
          else:
            assert response.status == (418 if body == b'http-error' else 201 if body != b'gzip' else 200)
            assert reply == (b'decoded\x00\xff' if body == b'gzip' else b'upstream\x00\xff' + body)
      params = fixture.peer.fixture.params / fixture.peer.fixture.prefix
      await anyio.Path(params / 'ClusterHud').write_bytes(b'1')
      count = len(received)
      async with client.post(fixture.url + '/stream', json={'cameras': ['road'], 'client_id': 'owned', 'sdp': 'abc', 'takeover': True}) as response:
        reply = await response.json()
        assert response.status == 409 and reply['code'] == 'cluster_hud_active' and len(received) == count
      await anyio.Path(params / 'ClusterHud').write_bytes(b'0')
      await runner.cleanup()
      async with client.post(fixture.url + '/stream', data=b'closed') as response:
        reply = await response.json()
        assert response.status == 502 and not reply['ok']
      await fixture.record('stream-gates.json', {'cluster_409': True, 'no_upstream_on_gate': True, 'unavailable_502': True})
  finally:
    await fixture.close()
    await runner.cleanup()


async def main() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  parser = argparse.ArgumentParser()
  for name in ['server', 'command', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--provider', choices=['source', 'native'], required=True)
  parser.add_argument('--fatal')
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)
