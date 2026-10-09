#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_settings_snapshot.py source|BINARY NEW_OUTPUT
from __future__ import annotations

import base64
from enum import StrEnum
import gzip
import json
import os
from pathlib import Path
import sys
from typing import Final, assert_never
import zlib

import anyio
from anyio.streams.buffered import BufferedByteReceiveStream
from carrot_server_dashcam_media import read_response
from carrot_server_dashcam_sync_probe import Json, Peer, startup
from carrot_server_system_actions import BINDING, ROOT

SOURCE: Final = Path(__file__).with_name('carrot_server_settings_snapshot_source.py')


class Coding(StrEnum):
  GZIP = 'gzip'
  DEFLATE = 'deflate'
  IDENTITY = ''


def setup(root: Path) -> None:
  root.mkdir(parents=True)
  (root / 'state').mkdir()
  (root / 'settings.json').write_text(
    json.dumps(
      {
        'apilot': 1,
        'params': [
          {'name': 'FutureSetting', 'group': 'Owned', 'min': 0, 'max': 20, 'default': 3},
          {'name': 'HiddenSetting', 'group': 'Owned', 'default': 4, 'hidden_brands': ['hyundai']},
        ],
      }
    )
  )
  (root / 'state/setting_favorites.json').write_text(json.dumps({'favorites': ['HiddenSetting', 'FutureSetting', 'FutureSetting', 'Unknown']}))
  (root / 'state/setting_unit_index.json').write_text(json.dumps({'units': {'FutureSetting': 3, 'Unknown': 99}}))
  (root / 'state/setting_profiles.json').write_text(
    json.dumps(
      {
        'profiles': [
          {'id': 'owned', 'name': 'Owned profile', 'values': {'FutureSetting': 8, 'HiddenSetting': 12, 'Unknown': 0}},
        ]
      }
    )
  )


def populate(root: Path) -> None:
  directory = root / 'params/d'
  for name, raw in {
    'FutureSetting': b'7',
    'HiddenSetting': b'99',
    'CarName': b'HYUNDAI IONIQ 5',
    'CarSelected3': b'owned-car',
    'GithubUsername': b'owner',
    'GithubSshKeys': b'ssh-ed25519 AQID fixture@owned\ninvalid',
    'IsMetric': b'1',
  }.items():
    (directory / name).write_bytes(raw)


async def fetch(peer: Peer, *, method: str = 'GET', coding: str = '') -> Json:
  async with await anyio.connect_tcp('127.0.0.1', peer.ready['port']) as stream:
    await stream.send(f'{method} /api/settings/snapshot HTTP/1.1\r\nHost: localhost\r\nAccept-Encoding: {coding}\r\nConnection: close\r\n\r\n'.encode())
    with anyio.fail_after(5):
      return await read_response(BufferedByteReceiveStream(stream), method)


def comparable(response: Json, root: Path) -> Json:
  body = base64.b64decode(response['body_base64'])
  if body and 'content-length' in response['headers']:
    assert len(body) == int(response['headers']['content-length'])
  coding = Coding(response['headers'].get('content-encoding', ''))
  match coding:
    case Coding.GZIP:
      body = gzip.decompress(body)
    case Coding.DEFLATE:
      body = zlib.decompress(body)
    case Coding.IDENTITY:
      pass
    case unexpected:
      assert_never(unexpected)
  text = body.decode().replace(str(root), '<owned>')
  try:
    data = json.loads(text) if body else None
  except json.JSONDecodeError:
    data = text
  headers = {name: response['headers'][name] for name in ['content-type', 'cache-control', 'allow', 'content-encoding', 'vary'] if name in response['headers']}
  return {'status': response['status'], 'body': data, 'headers': headers}


async def scenario(binary: str, output: Path, *, params: bool = True) -> None:
  environment = json.loads((ROOT / '.omo/evidence/carrot-server-225-resume/live-runtime/application-ruff-v4-invocation.json').read_text())
  names = ['source'] if binary == 'source' else ['source', 'native']
  peers = [Peer(output / name) for name in names]
  rows: list[Json] = []
  try:
    for peer in peers:
      root = peer.output
      await anyio.to_thread.run_sync(setup, root)
      config = {
        'root': str(root),
        'source': str(ROOT),
        'binding': str(BINDING),
        'params': params,
        'popular': {'ok': True, 'car_key': 'owned-car', 'popular_values': {'FutureSetting': 11}},
      }
      env = os.environ | {
        'PYTHONPATH': environment['PYTHONPATH'],
        'PARAMS_ROOT': str(root / 'params'),
        'OPENPILOT_PREFIX': 'd',
        'CARROT_DATA_DIR': str(root),
        'CARROT_SETTINGS_PATH': str(root / 'settings.json'),
      }
      command = [environment['argv'][0], '-P', str(SOURCE)] if peer.output.name == 'source' else [binary]
      await startup(peer, peer.start(command, config, env, True))
      if params:
        await anyio.to_thread.run_sync(populate, root)
    for name, method, coding in [('initial', 'GET', ''), ('head', 'HEAD', ''), ('gzip', 'GET', 'gzip'), ('deflate', 'GET', 'deflate'), ('method', 'POST', '')]:
      responses = [await fetch(peer, method=method, coding=coding) for peer in peers]
      values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
      assert all(value == values[0] for value in values), (name, values)
      if name == 'initial':
        body = values[0]['body']
        assert body['ok'] and body['settings']['has_params'] == params
        assert body['device_values']['DeviceType'] == 'pc'
        assert body['device_groups']['Developer'][-1] == 'GithubSshKeys'
        if params:
          assert body['values'] == {'FutureSetting': 7}
          assert body['favorites'] == ['FutureSetting']
          assert body['device_values']['GithubSshKeys'] == '1'
          assert body['device_ssh']['key_count'] == 1
          assert body['profiles'][0]['values'] == {'FutureSetting': 8, 'HiddenSetting': 12}
      rows.append({'name': name, 'equal': True, 'responses': responses})
    if params:
      for peer in peers:
        await anyio.Path(peer.output / 'params/d/FutureSetting').write_bytes(b'8')
      responses = [await fetch(peer) for peer in peers]
      values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
      assert all(value == values[0] for value in values) and values[0]['body']['values']['FutureSetting'] == 8
      histories = [json.loads(await anyio.Path(peer.output / 'state/param_changes.jsonl').read_text()) for peer in peers]
      assert all(history == histories[0] for history in histories)
      assert histories[0]['prev'] == 7 and histories[0]['next'] == 8 and histories[0]['source'] == 'device'
      rows.append({'name': 'drift', 'equal': True, 'responses': responses, 'history': histories})
    for peer in peers:
      await anyio.Path(peer.output / 'state/setting_favorites.json').write_text('{"favorites":["FutureSetting"]}')
      await anyio.Path(peer.output / 'state/setting_unit_index.json').write_text('{"units":{"FutureSetting":5,"OutsideCatalog":2}}')
      await anyio.Path(peer.output / 'state/setting_profiles.json').write_text('{"profiles":[]}')
    responses = [await fetch(peer) for peer in peers]
    values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
    assert all(value == values[0] for value in values)
    assert values[0]['body']['favorites'] == ['FutureSetting'] and values[0]['body']['profiles'] == []
    assert values[0]['body']['unit_index'] == {'FutureSetting': 5, 'OutsideCatalog': 2}
    rows.append({'name': 'live-state', 'equal': True, 'responses': responses})
    for peer in peers:
      for name in ['setting_favorites.json', 'setting_unit_index.json', 'setting_profiles.json']:
        await anyio.Path(peer.output / 'state' / name).write_text('{bad')
    responses = [await fetch(peer) for peer in peers]
    values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
    assert all(value == values[0] for value in values)
    assert values[0]['body']['favorites'] == [] and values[0]['body']['profiles'] == [] and values[0]['body']['unit_index'] == {}
    rows.append({'name': 'malformed-state', 'equal': True, 'responses': responses})
    for peer in peers:
      settings = peer.output / 'settings.json'
      stamp = settings.stat().st_mtime + 2
      await anyio.Path(settings).write_text('{bad')
      await anyio.to_thread.run_sync(os.utime, settings, (stamp, stamp))
    responses = [await fetch(peer) for peer in peers]
    values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
    assert all(value == values[0] for value in values) and values[0]['status'] == 500
    rows.append({'name': 'malformed-settings', 'equal': True, 'responses': responses})
    for peer in peers:
      await anyio.Path(peer.output / 'settings.json').unlink()
    responses = [await fetch(peer) for peer in peers]
    values = [comparable(response, peer.output) for response, peer in zip(responses, peers, strict=True)]
    assert all(value == values[0] for value in values) and values[0]['status'] == 404
    rows.append({'name': 'missing', 'equal': True, 'responses': responses})
    for peer in peers:
      await peer.stop()
      with anyio.fail_after(5):
        await peer.close()
    cleanup = [json.loads(await anyio.Path(peer.output / 'cleanup.json').read_text()) for peer in peers]
    assert all(row['exit'] == 0 and row['errors'] == [] for row in cleanup)
    await anyio.Path(output / 'result.json').write_text(
      json.dumps({'paired': len(peers) == 2, 'cases': len(rows), 'results': rows, 'cleanup': cleanup}, indent=2) + '\n'
    )
  finally:
    for peer in peers:
      if peer.process is not None and peer.process.returncode is None:
        with anyio.CancelScope(shield=True):
          await peer.close()


async def main(binary: str, output: Path) -> None:
  await anyio.Path(output).mkdir(exist_ok=False)
  await scenario(binary, output / 'available')
  await scenario(binary, output / 'unavailable', params=False)
  print('PASS')


if __name__ == '__main__':
  anyio.run(main, sys.argv[1], Path(sys.argv[2]).resolve())
