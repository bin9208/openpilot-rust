# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original complete YouTube feature with actual Cython Params and caller-owned recipients."""

from __future__ import annotations

import hashlib
import ipaddress
import json
import os
from pathlib import Path
import resource
import socket
import sys
from urllib.parse import urlsplit

import anyio
from aiohttp import web
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from original_params_binding import load


async def main() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  root = Path(str(await anyio.Path(config['owned_root']).resolve()))
  params_root = Path(str(await anyio.Path(config['params_root']).resolve()))
  assert params_root.is_relative_to(root)
  assert os.environ['PARAMS_ROOT'] == str(params_root)
  assert os.environ['OPENPILOT_PREFIX'] == config['prefix']
  assert os.environ['CARROT_DATA_DIR'] == str(root)
  endpoint = urlsplit(config['endpoint'])
  assert ipaddress.ip_address(endpoint.hostname).is_loopback
  source_modules()
  binding = Path(os.environ['ORIGINAL_PARAMS_BINDING'])
  extension, _logger = load(str(binding), f'ipc://{root}/owned-log', root / 'logs')
  from openpilot.selfdrive.carrot.server.features import youtube_live as routes
  from openpilot.selfdrive.carrot.server.services import youtube_live as service
  from openpilot.selfdrive.carrot.server.services import youtube_live_muxer, youtube_live_transport, youtube_live_writer

  # Replace only the fixed recipient constants at fixture startup; no runtime setting or key provider is invented.
  service.YOUTUBE_RTMPS_BASE = config['endpoint']
  service.YOUTUBE_RTMPS_HOST = endpoint.hostname
  service.YOUTUBE_RTMPS_PORT = endpoint.port
  params = extension.Params(str(params_root))
  assert params.get_param_path().startswith(str(params_root))
  app = web.Application(client_max_size=16 * 1024 * 1024)
  routes.register(app)
  if config.get('application'):
    web_root = root / 'web'
    await anyio.Path(web_root).mkdir(exist_ok=True)
    # Original app.py registers this GET/HEAD static fallback after feature routes.
    app.router.add_static('/', str(web_root), show_index=True)
  modules = [routes, service, youtube_live_muxer, youtube_live_transport, youtube_live_writer]
  source_files = {module.__file__: hashlib.sha256(await anyio.Path(module.__file__).read_bytes()).hexdigest() for module in modules}
  save(
    root / 'source-proof.json',
    {
      'providers': 'actual original service/routes, original Cython Params, C++msgq, PyAV and librtmp',
      'binding': {'path': str(binding), 'sha256': hashlib.sha256(await anyio.Path(binding).read_bytes()).hexdigest()},
      'owned_params_path': params.get_param_path(),
      'endpoint': config['endpoint'],
      'source_files': source_files,
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
    finally:
      await runner.cleanup()


if __name__ == '__main__':
  anyio.run(main)
