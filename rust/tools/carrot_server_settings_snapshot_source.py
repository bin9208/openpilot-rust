#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with the pinned source environment; stdin supplies the owned root and binding.
from __future__ import annotations

import asyncio  # Original aiohttp handler/to_thread cleanup uses asyncio unchanged.
import json
import os
from pathlib import Path
import sys
import types

from aiohttp import ClientSession, web
from original_params_binding import load


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  assert os.environ['PARAMS_ROOT'] == str(root / 'params')
  load(Path(config['binding']), f'ipc://{root}/logs.sock', root / 'logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import param_changes, params, popular_values

  params.HAS_PARAMS = config.get('params', True)
  params.Params = lambda: Params(str(root / 'params'))
  if params.HAS_PARAMS:
    params.Params()
  param_changes.time = types.SimpleNamespace(time=lambda: 1700000000)
  popular_values._popular_values_memory = config.get('popular')
  from openpilot.selfdrive.carrot.server.features import settings

  app = web.Application()
  session = ClientSession() if config.get('refresh', False) else None
  boot = None
  if session is not None:
    app['http'] = session
    boot = asyncio.create_task(popular_values.refresh_popular_values_once(session, upload=True))
  settings.register(app)
  runner = web.AppRunner(app, access_log=None)
  await runner.setup()
  try:
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    print(json.dumps({'ready': True, 'port': site._server.sockets[0].getsockname()[1]}), flush=True)
    while line := await asyncio.to_thread(sys.stdin.readline):
      if not line.strip() or json.loads(line).get('stop'):
        break
  finally:
    await runner.cleanup()
    for task in [boot, popular_values._popular_refresh_task]:
      if task is not None:
        task.cancel()
        try:
          await task
        except asyncio.CancelledError:
          continue
    if session is not None:
      await session.close()
  print(json.dumps({'stopped': True}), flush=True)


if __name__ == '__main__':
  asyncio.run(main())
