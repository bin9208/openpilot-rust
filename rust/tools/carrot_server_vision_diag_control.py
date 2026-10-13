#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import anyio
from aiohttp import ClientSession, web
from carrot_server_dashcam_upload import save
from carrot_server_terminal_cli_fixture import Inputs, Json
from carrot_server_terminal_http_fixture import Peer
from carrot_server_support_providers import prepare as prepare_support
from carrot_server_vision_diag_fixture import prepare
from carrot_server_vision_diag_cases import Mode, configure, run as run_case
from carrot_server_support_fixture import digest


async def run(args: argparse.Namespace) -> None:
  root = args.output.resolve()
  await anyio.Path(root).mkdir(parents=True)
  captures: list[Json] = []

  async def receive(request: web.Request) -> web.Response:
    fields = []
    reader = await request.multipart()
    async for part in reader:
      data = bytes(await part.read())
      if len(data) < 1_000_000:
        fields.append({'name': part.name, 'filename': part.filename, 'content_type': part.headers.get('Content-Type'), 'text': data.decode(), 'bytes': len(data)})
      else:
        decoded = data.decode()
        marker = decoded.find('# ===== DISCORD UPLOAD TRUNCATED')
        fields.append({'name': part.name, 'filename': part.filename, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'prefix': decoded[:256], 'tail': decoded[-256:], 'truncated_marker': decoded[marker:marker+150], 'replacement': '\ufffd' in decoded})
        assert marker > 0 and '\ufffd' in decoded and len(data) <= 8 * 1024 * 1024
    captures.append(fields)
    await anyio.to_thread.run_sync(save, root / 'multipart.json', captures)
    if args.mode is Mode.TIMEOUT:
      await anyio.sleep(21)
    return web.Response(status=429, text='owned rejected ' * 100) if args.mode is Mode.HTTP_ERROR else web.Response(status=204)

  receiver = web.Application()
  receiver.router.add_post('/hook', receive)
  receiver.router.add_post('/stream', lambda _request: web.Response(status=201, body=b'owned-stream'))
  runner = web.AppRunner(receiver)
  await runner.setup()
  listener = await anyio.create_tcp_listener(local_host='127.0.0.1', local_port=0)
  port = listener.extra(anyio.abc.SocketAttribute.local_address)[1]
  await listener.aclose()
  await web.TCPSite(runner, '127.0.0.1', port).start()
  peer = Peer(root, args.provider == 'source', Inputs(args.command, args.launcher, args.binding, args.vision_root), args.server, True)
  peer.source_program = Path(__file__).with_name('carrot_server_vision_diag_source.py')
  configure(args.mode, peer, f'http://127.0.0.1:{port}/hook')
  original_setup = peer.fixture.setup

  async def setup() -> None:
    await original_setup()
    await anyio.Path(root / 'bin').mkdir()
    await anyio.to_thread.run_sync(prepare_support, root, f'http://127.0.0.1:{port}/hook')
    config = json.loads(await anyio.Path(peer.fixture.config).read_text())
    await anyio.to_thread.run_sync(prepare, root, config['port'])
    if args.mode is Mode.ALIAS:
      await anyio.Path(peer.fixture.params / peer.fixture.prefix / 'CarrotVisionDiagDiscordWebhookUrl').write_text(f'http://127.0.0.1:{port}/hook')
    if args.mode is Mode.PARAM_READ:
      for key in ['IsOffroad', 'IsTakingSnapshot', 'CarParamsPersistent']:
        path = anyio.Path(peer.fixture.params / peer.fixture.prefix / key)
        if await path.is_file():
          await path.unlink()
        await path.mkdir()
    if args.mode is Mode.STATUS_ERROR:
      await anyio.Path(root / 'vision-state.json').write_text('{"runner_pid":"owned-invalid"}')
    if args.mode is Mode.LIVE_PROCESS:
      diagnostic = json.loads(await anyio.Path(root / 'diagnostic-config.json').read_text())
      diagnostic['proc_root'] = '/proc'
      diagnostic['process_matches'][-1][1] = str(peer.source_program if peer.source else args.server)
      await anyio.Path(root / 'diagnostic-config.json').write_text(json.dumps(diagnostic))
    if args.mode is Mode.HISTORY:
      await anyio.Path(root / 'stream-target.json').write_text(json.dumps({'target': f'http://127.0.0.1:{port}/stream'}))

  peer.fixture.setup = setup
  try:
    await peer.start()
    assert peer.process
    await anyio.to_thread.run_sync(save, root / 'identities.json', {'running_executable': await anyio.to_thread.run_sync(digest, Path(f'/proc/{peer.process.pid}/exe'))})
    if args.mode is Mode.CLOSED:
      await runner.cleanup()
    url = f'http://127.0.0.1:{peer.port}'
    async with ClientSession() as client:
      if await run_case(args.mode, client, peer):
        return
      async with client.get(url + '/api/vision_diag/server_snapshot') as response:
        body = await response.text()
        await anyio.to_thread.run_sync(save, root / 'snapshot-http.json', {'status': response.status, 'body': body})
        assert response.status == 200
        snapshot = json.loads(body)['snapshot']
        assert snapshot['params']['available'] and snapshot['params']['IsOffroad']
        assert len(snapshot['proc_net']['tcp']) == 320 and len(snapshot['sockets']['lines']) == 320
        assert len(snapshot['journal_tail']['lines']) == 240 and len(snapshot['vision_test']['log_tail']) == 240
        assert len(snapshot['processes']['camerad']) == 1 and snapshot['processes']['camerad'][0]['fd_count'] == 3
      async with client.post(url + '/api/vision_diag/upload_discord', json={'bundle': 'owned diagnostic\n한국어', 'filename': '../owned.txt', 'console': 'owned console', 'source': 'owned'}) as response:
        reply = await response.json()
        await anyio.to_thread.run_sync(save, root / 'upload-http.json', {'status': response.status, 'body': reply})
        assert response.status == 200 and reply['ok'] and reply['discord']['status'] == 204
        assert reply['discord']['filename'] == '.._owned.txt' and reply['discord']['console_filename'] == '.._owned_console.txt'
      assert len(captures) == 1 and len(captures[0]) == 3
      await anyio.to_thread.run_sync(save, root / 'result.json', {'snapshot': True, 'processes': True, 'line_bounds': True, 'multipart_upload': True, 'console': True})
  finally:
    with anyio.CancelScope(shield=True):
      await peer.close()
      await runner.cleanup()


async def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['server', 'command', 'launcher', 'binding', 'vision-root', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  parser.add_argument('--provider', choices=['source', 'native'], required=True)
  parser.add_argument('--mode', choices=list(Mode), type=Mode, default=Mode.BASIC)
  await run(parser.parse_args())


if __name__ == '__main__':
  anyio.run(main)
