#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# python -P rust/tools/carrot_server_settings_snapshot_constructor.py BINARY NEW_OUTPUT
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import sys
from typing import Literal

import anyio
from carrot_server_dashcam_sync_probe import Json, Peer, startup
from carrot_server_settings_snapshot import BINDING, ROOT, SOURCE, comparable, fetch, setup


async def scenario(binary: Path, output: Path, mode: Literal['removed', 'blocked']) -> None:
  environment = json.loads((ROOT / '.omo/evidence/carrot-server-225-resume/live-runtime/application-ruff-v4-invocation.json').read_text())
  peers = [Peer(output / name) for name in ['source', 'native']]
  observations: list[Json] = []
  try:
    for peer, command in zip(peers, [[environment['argv'][0], '-P', str(SOURCE)], [str(binary)]], strict=True):
      root = peer.output
      await anyio.to_thread.run_sync(setup, root)
      config = {'root': str(root), 'source': str(ROOT), 'binding': str(BINDING), 'params': True, 'popular': None}
      env = os.environ | {
        'PYTHONPATH': environment['PYTHONPATH'],
        'PARAMS_ROOT': str(root / 'params'),
        'OPENPILOT_PREFIX': 'd',
        'CARROT_DATA_DIR': str(root),
        'CARROT_SETTINGS_PATH': str(root / 'settings.json'),
      }
      await startup(peer, peer.start(command, config, env, True))
      namespace = (root / 'params/d').resolve()
      await anyio.to_thread.run_sync(shutil.rmtree, namespace)
      if mode == 'blocked':
        await anyio.Path(root / 'params').chmod(0o0)
      response = await fetch(peer)
      await anyio.Path(root / 'params').chmod(0o755)
      observations.append({'response': response, 'directory_created': await anyio.Path(root / 'params/d').is_dir()})
    values = [comparable(row['response'], peer.output) for row, peer in zip(observations, peers, strict=True)]
    equal = values[0] == values[1] and observations[0]['directory_created'] == observations[1]['directory_created']
    await anyio.Path(output / 'result.json').write_text(
      json.dumps({'mode': mode, 'equal': equal, 'observations': observations, 'comparable': values}, indent=2) + '\n'
    )
    assert equal, values
    for peer in peers:
      await peer.stop()
      with anyio.fail_after(5):
        await peer.close()
  finally:
    for peer in peers:
      if await anyio.Path(peer.output / 'params').is_dir():
        await anyio.Path(peer.output / 'params').chmod(0o755)
      if peer.process is not None and peer.process.returncode is None:
        with anyio.CancelScope(shield=True):
          await peer.close()


async def main(binary: Path, output: Path) -> None:
  await anyio.Path(output).mkdir(exist_ok=False)
  errors = []
  for name in ['removed', 'blocked']:
    try:
      await scenario(binary, output / name, name)
    except AssertionError as failure:
      errors.append(str(failure))
  await anyio.Path(output / 'result.json').write_text(json.dumps({'equal': not errors, 'errors': errors}, indent=2) + '\n')
  assert not errors, errors
  print('PASS')


if __name__ == '__main__':
  anyio.run(main, Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
