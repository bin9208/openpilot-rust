#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_system_http.py BINARY NEW_OUTPUT
from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import sys
import time
from typing import Final

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer, startup
from carrot_server_system_actions import BINDING, ROOT

SOURCE: Final = Path(__file__).with_name('carrot_server_system_http_source.py')
RECIPIENT: Final = Path(__file__).with_name('carrot_server_system_recipient.py')


def save(path: Path, value: Json) -> None:
  path.write_text(json.dumps(value, indent=2) + '\n')


async def fetch(peer: Peer, path: str, method: str = 'GET', body: bytes = b'', charset: str = 'utf-8') -> Json:
  async with await anyio.connect_tcp('127.0.0.1', peer.ready['port']) as stream:
    headers = f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json; charset={charset}\r\n'
    await stream.send((headers + f'Content-Length: {len(body)}\r\nConnection: close\r\n\r\n').encode() + body)
    with anyio.fail_after(5):
      return await read_response(BufferedByteReceiveStream(stream), method)


def comparable(response: Json, root: Path) -> Json:
  raw = base64.b64decode(response['body_base64'])
  headers = response['headers']
  value = raw.decode().replace(str(root / 'localtime'), '/data/etc/localtime')
  value = value.replace(str(root / 'zones'), '/usr/share/zoneinfo').replace(str(root), '<owned>')
  fields = {key: headers[key] for key in ['content-type', 'content-length', 'allow'] if key in headers}
  if raw and value != raw.decode():
    assert int(headers['content-length']) == len(raw)
    fields['content-length'] = str(len(value.encode()))
  return {'status': response['status'], 'body': value, 'headers': fields}


def setup(root: Path, python: str) -> None:
  root.mkdir(parents=True)
  (root / 'bin').mkdir()
  for name in ['nmcli', 'sudo']:
    (root / 'bin' / name).symlink_to(RECIPIENT)
  (root / 'bin/python3').symlink_to(python)
  (root / 'zones').mkdir()
  (root / 'zones/UTC').write_bytes(b'owned-zone')
  (root / 'zones/Other').write_bytes(b'owned-zone-2')
  (root / 'localtime').symlink_to(root / 'zones/UTC')
  (root / 'offroad').mkdir()
  (root / 'offroad/fcc.html').write_bytes(b'<p>owned\xff\r\nregulatory</p>\r')
  (root / 'settings.json').write_text(json.dumps({'params': [{'name': 'FutureSetting', 'min': 0, 'max': 60, 'default': 20}]}))
  (root / 'hold-network').touch()


async def started(root: Path, count: int) -> list[Json]:
  with anyio.fail_after(2):
    while True:
      path = root / 'commands.jsonl'
      lines = await anyio.Path(path).read_text() if await anyio.Path(path).exists() else ''
      rows = [json.loads(line) for line in lines.splitlines()]
      if len(rows) >= count:
        return rows
      await anyio.sleep(0.005)


async def scenario(binary: Path, output: Path, *, engaged: bool = False, params: bool = True, stop_held: bool = False) -> None:
  await anyio.Path(output).mkdir()
  environment = json.loads((ROOT / '.omo/evidence/carrot-server-225-resume/live-runtime/application-ruff-v4-invocation.json').read_text())
  peers = [Peer(output / name) for name in ['source', 'native']]
  observations: list[Json] = []
  try:
    for peer, command in zip(peers, [[environment['argv'][0], '-P', str(SOURCE)], [str(binary)]], strict=True):
      root = peer.output
      await anyio.to_thread.run_sync(setup, root, environment['argv'][0])
      config = {'mode': 'server', 'root': str(root), 'source': str(ROOT), 'binding': str(BINDING), 'params': params, 'engaged': engaged}
      env = os.environ | {
        'PYTHONPATH': environment['PYTHONPATH'],
        'PATH': str(root / 'bin'),
        'PARAMS_ROOT': str(root / 'params'),
        'OPENPILOT_PREFIX': 'd',
        'SYSTEM_FIXTURE_ROOT': str(root),
        'CARROT_DATA_DIR': str(root),
        'CARROT_SETTINGS_PATH': str(root / 'settings.json'),
      }
      await startup(peer, peer.start(command, config, env, True))
      await started(root, 1)
    if stop_held:
      before = [await started(peer.output, 1) for peer in peers]
      begin = time.monotonic()
      for peer in peers:
        await peer.stop()
      await anyio.sleep(0.1)
      assert all(peer.process.returncode is None for peer in peers)
      for peer in peers:
        with anyio.fail_after(5):
          await peer.close()
      observations.append({'stopped_while_held': True, 'seconds': time.monotonic() - begin, 'commands_before_stop': before})
    else:
      for peer in peers:
        begin = time.monotonic()
        response = await fetch(peer, '/api/device_network')
        assert time.monotonic() - begin < 1
        assert response['status'] == 200
        observations.append({'cached_during_held': True, 'response': response})
        await anyio.Path(peer.output / 'hold-network').unlink()
      for peer in peers:
        await started(peer.output, 2)
      cases = [
        ('GET', '/api/device_network?force=1', b'', 'utf-8'),
        ('HEAD', '/api/device_network', b'', 'utf-8'),
        ('POST', '/api/device_network', b'', 'utf-8'),
        ('GET', '/api/calibration_status', b'', 'utf-8'),
        ('GET', '/api/regulatory', b'', 'utf-8'),
        ('HEAD', '/api/regulatory', b'', 'utf-8'),
        ('POST', '/api/reboot', b'', 'utf-8'),
        ('GET', '/api/reboot', b'', 'utf-8'),
        ('POST', '/api/poweroff', b'', 'utf-8'),
        ('POST', '/api/recalibrate', b'', 'utf-8'),
        ('POST', '/api/set_default', b'', 'utf-8'),
        ('POST', '/api/time_sync', b'{bad', 'utf-8'),
        ('POST', '/api/time_sync', b'{}', 'utf-8'),
        ('POST', '/api/time_sync', b'null', 'utf-8'),
        ('POST', '/api/time_sync', b'{"epoch_ms":0,"timezone":1}', 'utf-8'),
        ('POST', '/api/time_sync', b'{"epoch_ms":1700000010000}', 'utf-8'),
        ('POST', '/api/time_sync', json.dumps({'epoch_ms': 1700000011000, 'timezone': 'Other'}).encode('utf-16'), 'utf-16'),
        ('POST', '/api/time_sync', b'{"epoch_ms":0,"timezone":"Missing"}', 'utf-8'),
      ]
      for method, path, body, charset in cases:
        responses = [await fetch(peer, path, method, body, charset) for peer in peers]
        equal = comparable(responses[0], peers[0].output) == comparable(responses[1], peers[1].output)
        observations.append({'method': method, 'path': path, 'equal': equal, 'responses': responses})
        await anyio.to_thread.run_sync(save, output / 'observations.json', observations)
        assert equal, (path, [comparable(row, peer.output) for row, peer in zip(responses, peers, strict=True)])
      for peer in peers:
        await peer.stop()
        with anyio.fail_after(5):
          await peer.close()
    rows = [json.loads(await anyio.Path(peer.output / 'cleanup.json').read_text()) for peer in peers]
    assert all(row['exit'] == 0 for row in rows), rows
    commands = [await started(peer.output, 2) for peer in peers]
    for captured in commands:
      for row in captured:
        assert not await anyio.Path(f'/proc/{row["pid"]}').exists()
    await anyio.to_thread.run_sync(
      save,
      output / 'result.json',
      {'paired_http': sum('equal' in row for row in observations), 'equal': True, 'observations': observations, 'cleanup': rows, 'owned_children_reaped': True},
    )
  finally:
    for peer in peers:
      if peer.process is not None and peer.process.returncode is None:
        with anyio.CancelScope(shield=True):
          await peer.close()


async def main(binary: Path, output: Path) -> None:
  await anyio.Path(output).mkdir(exist_ok=False)
  await scenario(binary, output / 'ordinary')
  await scenario(binary, output / 'engaged', engaged=True)
  await scenario(binary, output / 'unavailable', params=False)
  await scenario(binary, output / 'held-stop', stop_held=True)
  print('PASS')


if __name__ == '__main__':
  anyio.run(main, Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
