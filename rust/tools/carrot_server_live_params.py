#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_live_params.py --binary PATH --params-binding EXISTING_SO --output NEW_DIR
# Caller supplies original dependencies. Only affected typed Params broker extraction is compared.
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import uuid

import anyio
from carrot_server_dashcam_sync_probe import Peer, startup
from carrot_server_dashcam_upload import save
from carrot_server_live_wire import body, fetch, normalize


async def run(binary: Path, binding: Path, output: Path) -> None:
  namespace = 'rust-probe-live-params-' + uuid.uuid4().hex
  queue = anyio.Path('/dev/shm', 'msgq_' + namespace)
  await queue.mkdir()
  params = anyio.Path(output / 'params' / namespace)
  await params.mkdir(parents=True)
  await (params / 'IsMetric').write_bytes(b'1')
  await (params / 'ShowPathMode').write_bytes(b' +001_2 ')
  env = {
    **os.environ,
    'OPENPILOT_PREFIX': namespace,
    'PARAMS_ROOT': str(output / 'params'),
    'CARROT_DATA_DIR': str(output / 'data'),
    'ORIGINAL_PARAMS_BINDING': str(binding),
  }
  peers = [Peer(output / name) for name in ('source', 'native')]
  for peer in peers:
    peer.output.mkdir()
  rows = []
  cache_head = []
  try:
    commands = ([sys.executable, '-P', str(Path(__file__).with_name('carrot_server_live_source.py'))], [str(binary)])
    for peer, command in zip(peers, commands, strict=True):
      await startup(peer, peer.start(command, {'params': True, 'unavailable': False, 'output': str(peer.output)}, env, True))
    for name in ('canonical', 'invalid', 'bool-read-error'):
      if name == 'invalid':
        await (params / 'ShowPathMode').write_bytes(b'owned\xffnumeric')
      elif name == 'bool-read-error':
        await (params / 'IsMetric').chmod(0)
      await anyio.sleep(0.13)
      responses = [await fetch(peer, '/api/live_runtime?force=1') for peer in peers]
      rows.append({'case': name, 'responses': responses})
      save(output / 'observations.json', rows)
      if name == 'canonical':
        cached = [await fetch(peer, '/api/live_runtime?force=1') for peer in peers]
        assert [body(row)['meta']['generatedAtMs'] for row in cached] == [body(row)['meta']['generatedAtMs'] for row in responses]
        assert normalize(body(cached[0])) == normalize(body(cached[1]))
        heads = [await fetch(peer, '/api/live_runtime', 'HEAD') for peer in peers]
        assert [row['status'] for row in heads] == [200, 200]
        assert [row['body_base64'] for row in heads] == ['', '']
        cache_head.append({'cached_force': cached, 'head': heads})
        save(output / 'cache-head.json', cache_head)
    assert all(normalize(body(row['responses'][0])) == normalize(body(row['responses'][1])) for row in rows)
    values = [body(row['responses'][0])['runtime']['params'] for row in rows]
    assert values[0]['ShowPathMode'] == '12'
    assert values[1]['ShowPathMode'] is None
    assert values[2]['IsMetric'] == '0'
    save(output / 'result.json', {'params_pairs': 3, 'cache_head_pairs': 2, 'canonical': '12', 'invalid': None, 'bool_read_error': '0'})
  finally:
    failed = sys.exc_info()[0] is not None
    await (params / 'IsMetric').chmod(0o600)
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


async def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--params-binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  await anyio.Path(output).mkdir(parents=True)
  await run(args.binary.resolve(), args.params_binding.resolve(), output)
  print(json.dumps({'status': 'PASS', 'pairs': 3}))


if __name__ == '__main__':
  anyio.run(main)
