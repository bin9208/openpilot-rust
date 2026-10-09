#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run with the existing source environment; stdin supplies only owned paths.
"""Drive unchanged original QR HTTP handlers with the real Cython Params binding."""

from __future__ import annotations

import asyncio  # Unchanged original aiohttp handlers/cleanup require their source scheduler.
import json
import os
from pathlib import Path
import sys

from aiohttp import web
from original_params_binding import load


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  assert os.environ['PARAMS_ROOT'] == str(root / 'params')
  load(Path(config['binding']), f'ipc://{root}/logs.sock', root / 'logs')
  from openpilot.common.params import Params
  from openpilot.selfdrive.carrot.server.services import params

  params.Params = lambda: Params(str(root / 'params'))
  params.Params()
  from openpilot.selfdrive.carrot.server.features import params as feature

  app = web.Application()
  feature.register(app)
  runner = web.AppRunner(app, access_log=None)
  await runner.setup()
  try:
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    print(json.dumps({'ready': True, 'port': site._server.sockets[0].getsockname()[1]}), flush=True)
    await asyncio.to_thread(sys.stdin.readline)
  finally:
    await runner.cleanup()
  print(json.dumps({'stopped': True}), flush=True)


if __name__ == '__main__':
  asyncio.run(main())
