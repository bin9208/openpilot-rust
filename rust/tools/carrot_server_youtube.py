#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Focused actual source/native YouTube API and persisted state controls; dependencies supplied by caller."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource

import anyio
from carrot_server_dashcam_upload import save
from carrot_server_youtube_fixture import Fixture
from carrot_server_youtube_transport import certificate


async def boundary(fixture: Fixture) -> None:
  base = '/api/youtube_live'
  await fixture.pair('initial-status', base + '/status', ('GET', b'', 200))
  await fixture.pair('initial-status-head', base + '/status', ('HEAD', b'', 200))
  await fixture.pair('initial-diagnostics', base + '/diagnostics', ('GET', b'', 200))
  await fixture.pair('initial-key', base + '/stream_key', ('GET', b'', 200))
  await fixture.pair('key-head', base + '/stream_key', ('HEAD', b'', 200))
  for name, body, status in [
    ('empty', b'', 400),
    ('malformed', b'{', 400),
    ('null', b'null', 500),
    ('array', b'[]', 500),
    ('missing', b'{}', 400),
    ('null-primary', b'{"stream_key":null,"key":"ignore-this-alias"}', 400),
  ]:
    await fixture.pair('set-' + name, base + '/stream_key', ('POST', body, status))
  await fixture.pair('numeric-alias', base + '/stream_key', ('POST', b'{"key":12345678}', 200))
  await fixture.pair('numeric-key-read', base + '/stream_key', ('GET', b'', 200))
  await fixture.pair('url-extraction', base + '/stream_key', ('POST', b'{"stream_key":" rtmps://ignored.invalid/live2/owned-key-1234567/// "}', 200))
  for peer in fixture.peers:
    secret = peer.root / 'state/youtube_live_secret.json'
    stat = await anyio.Path(secret).stat()
    assert stat.st_mode & 0o777 == 0o600
  await fixture.pair('set-boolean', base + '/stream_key', ('POST', b'{"key":true}', 200))
  assert all(not peer.probe.observations for peer in fixture.peers)
  await fixture.pair('test-short-key-still-ready', base + '/test', ('POST', b'{ignored-body', 200))
  await fixture.pair('validate-short-key', base + '/stream_key/validate', ('POST', b'', 409))
  await fixture.pair('validate-valid-override', base + '/stream_key/validate', ('POST', b'{"key":"owned-key-1234567"}', 200))
  await fixture.pair('validate-array-stored-fallback', base + '/stream_key/validate', ('POST', b'[]', 409))
  await fixture.pair('validate-invalid-json', base + '/stream_key/validate', ('POST', b'{', 400))
  await fixture.pair('clear-key', base + '/stream_key', ('DELETE', b'', 200))
  await fixture.pair('clear-again', base + '/stream_key', ('DELETE', b'', 200))
  await fixture.pair('test-empty-key', base + '/test', ('POST', b'', 409))
  await fixture.pair('status-post', base + '/status', ('POST', b'', 405))
  await fixture.pair('key-put', base + '/stream_key', ('PUT', b'', 405))
  await anyio.sleep(0.55)
  for peer in fixture.peers:
    await anyio.Path(peer.params / peer.prefix / 'CarrotYouTubeQuality').write_bytes(b'3junk')
    await anyio.Path(peer.params / peer.prefix / 'CarrotYouTubeLive').write_bytes(b'2')
    await anyio.Path(peer.params / peer.prefix / 'CarrotYouTubeTimestamp').write_bytes(b'1')
  await anyio.sleep(0.55)
  values = await fixture.pair('actual-params-integer-prefix-bool-equality', base + '/status', ('GET', b'', 200))
  assert all(value['quality'] == 'wide' and value['requested_quality'] == 3 and not value['enabled'] and value['timestamp_caption_enabled'] for value in values)
  await fixture.pair('actual-params-cache-head', base + '/status', ('HEAD', b'', 200))
  await fixture.pair('actual-params-diagnostics', base + '/diagnostics', ('GET', b'', 200))
  await anyio.sleep(0.1)
  assert all(len(peer.probe.observations) == 5 and all(row['tls'] for row in peer.probe.observations) for peer in fixture.peers)


async def persisted(fixture: Fixture) -> None:
  values = await fixture.pair('persisted-raw-signed-counter', '/api/youtube_live/status', ('GET', b'', 200))
  assert all(value['bytes_sent'] == -17 and value['total_mb'] == 0 and value['restart_count'] == 0 for value in values)
  await fixture.pair('persisted-head', '/api/youtube_live/status', ('HEAD', b'', 200))


async def main() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = Path(str(await anyio.Path(args.output).resolve()))
  await anyio.Path(output).mkdir(parents=True)
  os.environ['ORIGINAL_PARAMS_BINDING'] = str(await anyio.Path(args.binding).resolve())
  certs = await anyio.to_thread.run_sync(certificate, output, 'trusted', '127.0.0.1')
  for scenario in ['boundary', 'persisted']:
    directory = output / scenario
    await anyio.Path(directory).mkdir()
    fixture = Fixture(directory, Path(str(await anyio.Path(args.binary).resolve())), certs)
    try:
      if scenario == 'persisted':
        for peer in fixture.peers:
          await anyio.Path(peer.root / 'state').mkdir(parents=True)
          await anyio.Path(peer.root / 'state/youtube_live.json').write_text(json.dumps({'bytes_sent': -17, 'restart_count': 'bad-restart'}))
      await fixture.start()
      if scenario == 'boundary':
        await boundary(fixture)
      else:
        await persisted(fixture)
    finally:
      with anyio.CancelScope(shield=True):
        await fixture.close()
  await anyio.to_thread.run_sync(
    save, output / 'result.json', {'http_pairs': 28, 'persisted_pairs': 2, 'actual_cython_params': True, 'health_probe_count_per_peer': 5, 'pass': True}
  )


if __name__ == '__main__':
  anyio.run(main)
