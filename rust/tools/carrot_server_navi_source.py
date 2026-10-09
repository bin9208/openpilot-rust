# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Actual original Navi routes; caller supplies dependencies and ORIGINAL_PARAMS_BINDING.
from __future__ import annotations

import ast
import hashlib
import inspect
import json
import os
from pathlib import Path
import socket
import sys
import traceback

import anyio
from aiohttp import web
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from original_params_binding import load


async def main() -> None:
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  output = Path(config['output'])
  source_modules()
  binding = os.environ['ORIGINAL_PARAMS_BINDING']
  extension, _ = load(binding, f'ipc://{output}/owned-log', output / 'logs')
  from openpilot.selfdrive.carrot.server.features.carrot_navi import bridge, routes

  bridge.HAS_PARAMS = config['params']
  bridge.Params = (lambda: extension.Params(os.environ['PARAMS_ROOT'])) if config['params'] else None
  app = web.Application(client_max_size=16 * 1024 * 1024)
  routes.register(app)
  if config.get('composed'):
    path = Path('openpilot/selfdrive/carrot/server/app.py')
    text = await anyio.Path(path).read_text()
    cleanup = next(node for node in ast.parse(text).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup')
    scope = {'web': web, 'asyncio': sys.modules['asyncio'], 'traceback': traceback}
    exec(compile(ast.Module(body=[cleanup], type_ignores=[]), str(path), 'exec'), scope)
    app.on_cleanup.append(scope['on_cleanup'])
  runner = web.AppRunner(app)
  with socket.socket() as listener:
    try:
      await runner.setup()
      if config['unavailable']:
        app[routes.APP_KEY] = None
      listener.bind(('127.0.0.1', 0))
      listener.listen()
      listener.setblocking(False)
      await web.SockSite(runner, listener).start()
      files = [Path(bridge.__file__).with_name(name + '.py') for name in ('bridge', 'client_hub', 'media_pipeline', 'fmp4', 'protocol', 'routes')]
      save(
        output / 'source-proof.json',
        {
          'source_files': {str(path): hashlib.sha256(await anyio.Path(path).read_bytes()).hexdigest() for path in files},
          'params_binding': {'path': binding, 'sha256': hashlib.sha256(await anyio.Path(binding).read_bytes()).hexdigest()},
          'websocket_close_default_seconds': inspect.signature(web.WebSocketResponse).parameters['timeout'].default,
          'providers': 'unchanged original routes/bridge, actual Cython Params, actual C++msgq and PyAV',
        },
      )
      print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}), flush=True)
      await anyio.to_thread.run_sync(sys.stdin.readline)
      if config.get("composed"):
        print(json.dumps({"stopping": True}), flush=True)
    finally:
      await runner.cleanup()


if __name__ == '__main__':
  anyio.run(main)
