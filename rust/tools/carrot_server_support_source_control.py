#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import base64
import json
from pathlib import Path
import re

import anyio
from aiohttp import ClientSession, ClientWebSocketResponse, web
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cleanup import close
from carrot_server_terminal_cli_fixture import Inputs, Json
from carrot_server_terminal_http_fixture import Peer
from carrot_server_terminal_pty import output
from carrot_server_support_providers import prepare


async def packet(socket: ClientWebSocketResponse, kind: str) -> dict[str, Json]:
  with anyio.fail_after(6):
    while True:
      data = await socket.receive_json()
      if data['type'] == kind:
        return data


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  received = []

  async def webhook(request: web.Request) -> web.Response:
    body = await request.json()
    received.append(body)
    await anyio.to_thread.run_sync(save, root / 'webhook.json', received)
    return web.Response(status=204)

  receiver = web.Application()
  receiver.router.add_post('/hook', webhook)
  receiver_runner = web.AppRunner(receiver)
  await receiver_runner.setup()
  listener = await anyio.create_tcp_listener(local_host='127.0.0.1', local_port=0)
  port = listener.extra(anyio.abc.SocketAttribute.local_address)[1]
  await listener.aclose()
  await web.TCPSite(receiver_runner, '127.0.0.1', port).start()
  inputs = Inputs(args.native, args.launcher, args.binding, args.vision_root)
  peer = Peer(root / 'source', True, inputs, args.native, False)
  peer.source_program = Path(__file__).with_name('carrot_server_support_source.py')
  original_start = peer.fixture.setup

  async def setup() -> None:
    await original_start()
    await anyio.Path(peer.root / 'bin').mkdir()
    await anyio.to_thread.run_sync(prepare, peer.root, f'http://127.0.0.1:{port}/hook')

  peer.fixture.setup = setup
  try:
    await peer.start()
    url = f'http://127.0.0.1:{peer.port}'
    async with ClientSession() as client:
      async with client.ws_connect(url + '/ws/terminal_pty') as local:
        local_meta = await local.receive_json()
        await local.send_json({'type': 'input', 'data': "printf 'OWNED_LOCAL\\n'"})
        await output(local, b'OWNED_LOCAL\r\n')
        async with client.post(url + '/api/support_terminal/start', json={'note': 'owned issue', 'ttl_seconds': 900}) as response:
          session = await response.json()
          assert response.status == 200 and session['state'] == 'sharing' and session['ttl_seconds'] == 900
        assert len(received) == 1
        matched = re.search(r'PIN: \*\*__(\d{6})__\*\*', received[0]['content'])
        assert matched
        pin = matched.group(1)
        async with client.ws_connect(url + '/ws/support_terminal/owner') as owner:
          await packet(owner, 'session_status')
          guest_url = session['local_url'].split('/support/terminal/')[0]
          async with client.ws_connect(guest_url + '/ws/support_terminal/' + session['id']) as guest:
            await guest.send_json({'type': 'auth', 'pin': pin})
            await packet(guest, 'auth_ok')
            guest_meta = await packet(guest, 'meta')
            assert guest_meta['pid'] == local_meta['pid'] and not guest_meta['primary']
            await guest.send_json({'type': 'input', 'data': "printf 'OWNED_APPROVED\\n'"})
            requested = await packet(owner, 'command_request')
            await packet(guest, 'command_waiting_approval')
            async with client.post(url + f'/api/support_terminal/commands/{requested["id"]}/approve') as response:
              assert response.status == 200 and (await response.json())['ok']
            transcript = bytearray()
            running = False
            with anyio.fail_after(6):
              while not running or b'OWNED_APPROVED\r\n' not in transcript:
                message = await guest.receive_json()
                if message['type'] == 'pty_output':
                  transcript.extend(base64.b64decode(message['b64']))
                elif message['type'] == 'command_running':
                  running = True
        await local.close()
      assert peer.process and peer.process.stdin and peer.reader
      await peer.process.stdin.send(b'cleanup\n')
      with anyio.fail_after(6):
        cleanup = json.loads(await peer.reader.receive_until(b'\n', 65536))
      assert cleanup['pty']['alive'] and cleanup['support']['active'] and cleanup['guest_origin_alive'] and cleanup['tunnel_child_alive']
      async with client.get(session['local_url']) as response:
        assert response.status == 200
      await anyio.to_thread.run_sync(
        save,
        root / 'result.json',
        {
          'start': session,
          'local_meta': local_meta,
          'guest_meta': guest_meta,
          'approved_command': requested,
          'app_cleanup': cleanup,
          'guest_page_survives_main_cleanup': True,
        },
      )
  finally:
    with anyio.CancelScope(shield=True):
      cleanup_errors = []
      child_exit = None
      try:
        await peer.close()
      except Exception as error:
        cleanup_errors.append(f'peer: {error!r}')
      try:
        cloud_record = anyio.Path(peer.root / 'cloud-child.json')
        if await cloud_record.exists():
          child = json.loads(await cloud_record.read_text())
          child_exit = await anyio.to_thread.run_sync(close, child['pid'], child['starttime'])
      except Exception as error:
        cleanup_errors.append(f'cloud child: {error!r}')
      try:
        await receiver_runner.cleanup()
      except Exception as error:
        cleanup_errors.append(f'receiver: {error!r}')
      await anyio.to_thread.run_sync(save, root / 'cleanup.json', {'errors': cleanup_errors, 'child_exit': child_exit})


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['native', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  await anyio.Path(args.output.resolve()).mkdir(parents=True)
  await run(args)


if __name__ == '__main__':
  anyio.run(main)
