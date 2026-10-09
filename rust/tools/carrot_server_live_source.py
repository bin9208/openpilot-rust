# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller supplies original dependencies and ORIGINAL_PARAMS_BINDING, an existing Cython Params module.
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import traceback
from types import SimpleNamespace

import anyio
from aiohttp import web
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from original_params_binding import load


async def main() -> None:
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  source_modules()
  output = Path(config['output'])
  binding = Path(os.environ['ORIGINAL_PARAMS_BINDING'])
  binding_sha = hashlib.sha256(await anyio.Path(binding).read_bytes()).hexdigest()
  extension, _ = load(str(binding), f'ipc://{output}/owned-log', output / 'logs')
  from openpilot.cereal import messaging
  from openpilot.selfdrive.carrot.server.live_runtime import broker
  from openpilot.selfdrive.carrot.server.live_runtime.normalize import to_transport_safe
  from openpilot.selfdrive.carrot.server.features import ws
  from openpilot.selfdrive.carrot.realtime.transports import CameraWsHub, RawWsHub

  broker.Params = (lambda: extension.Params(os.environ['PARAMS_ROOT'])) if config['params'] else None
  app = web.Application()
  try:
    app['realtime_broker'] = broker.RealtimeBroker(
      repo_flavor='c3',
      include_optional=('navInstructionCarrot', 'navRoute'),
      exclude_services=('carState', 'controlsState', 'longitudinalPlan', 'liveCalibration', 'modelV2', 'roadCameraState', 'deviceState'),
    )
    app['realtime_broker_error'] = None
  except (OSError, RuntimeError, ValueError) as error:
    app['realtime_broker'] = None
    app['realtime_broker_error'] = str(error)
  app['realtime_broker_poll_lock'] = sys.modules['asyncio'].Lock()
  app['realtime_raw_hub'] = RawWsHub(messaging)
  app['realtime_camera_hub'] = CameraWsHub(messaging)
  if config['unavailable']:
    app['realtime_broker'] = app['realtime_raw_hub'] = app['realtime_camera_hub'] = None
  path = Path('openpilot/selfdrive/carrot/server/features/system.py')
  text = await anyio.Path(path).read_text()
  names = {'_select_live_runtime_services', 'is_drive_engaged', 'api_live_runtime'}
  nodes = [node for node in ast.parse(text).body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in names]
  scope = {
    'web': web,
    'RealtimeBroker': broker.RealtimeBroker,
    'asyncio': sys.modules['asyncio'],
    'to_transport_safe': to_transport_safe,
    '_LIVE_RUNTIME_SERVICE_NAMES': ('navInstructionCarrot', 'navRoute'),
    '_LIVE_RUNTIME_CACHE_MAX_AGE_MS': 4800,
    '_LIVE_RUNTIME_FORCE_MIN_AGE_MS': 100,
  }
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
  app.router.add_get('/api/live_runtime', scope['api_live_runtime'])
  ws.register(app)
  app_path = Path('openpilot/selfdrive/carrot/server/app.py')
  app_text = await anyio.Path(app_path).read_text()
  cleanup_node = next(node for node in ast.parse(app_text).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup')
  cleanup_scope = {'web': web, 'asyncio': sys.modules['asyncio'], 'traceback': traceback}
  exec(compile(ast.Module(body=[cleanup_node], type_ignores=[]), str(app_path), 'exec'), cleanup_scope)
  app.on_cleanup.append(cleanup_scope['on_cleanup'])

  async def engaged(request: web.Request) -> web.Response:
    return web.json_response({'engaged': scope['is_drive_engaged'](request)})

  app.router.add_get('/owned/engaged', engaged)
  save(
    output / 'source-proof.json',
    {
      'handler_functions': {node.name: hashlib.sha256(ast.get_source_segment(text, node).encode()).hexdigest() for node in nodes},
      'providers': 'actual original broker/RawWsHub/CameraWsHub, C++msgq and original Cython Params binding',
      'params_binding': {'path': str(binding), 'sha256': binding_sha},
      'source_files': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [path, Path(ws.__file__), Path(broker.__file__)]},
    },
  )
  runner = web.AppRunner(app)
  with socket.socket() as listener:
    try:
      await runner.setup()
      listener.bind(('127.0.0.1', 0))
      listener.listen()
      listener.setblocking(False)
      await web.SockSite(runner, listener).start()
      print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}), flush=True)
      await anyio.to_thread.run_sync(sys.stdin.readline)
      if config.get('composed'):
        print(json.dumps({'stopping': True, 'engaged': scope['is_drive_engaged'](SimpleNamespace(app=app))}), flush=True)
    finally:
      await runner.cleanup()


if __name__ == '__main__':
  anyio.run(main)
