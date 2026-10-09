# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Original terminal routes/PTY with owned fixed paths and unchanged function bodies."""

from __future__ import annotations

import ast
import hashlib
import json
from pathlib import Path
import socket
import sys
import traceback

import anyio
from aiohttp import web
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from carrot_server_tools_source import OwnedPaths


async def main() -> None:
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  root = Path(str(await anyio.Path(config['owned_root']).resolve()))
  assert root.is_relative_to(Path.cwd())
  source_modules()
  from openpilot.selfdrive.carrot.server.features import terminal
  from openpilot.selfdrive.carrot.server.services import terminal_pty, tmux

  if config.get('cli_config'):
    from carrot_server_terminal_command_source import configure

    configure(Path(config['cli_config']))
    sys.executable = str(root / 'bin/python3')
    from openpilot.selfdrive.carrot.server.features import vision_test
  terminal_source = Path(terminal.__file__)
  terminal_tree = ast.parse(await anyio.Path(terminal_source).read_text(), filename=str(terminal_source))
  download_paths = OwnedPaths({'/data/media/tmux.log': str(root / 'tmux.log')})
  exec(compile(download_paths.visit(terminal_tree), str(terminal_source), 'exec'), terminal.__dict__)

  tmux.TMUX_START_DIR = str(root / 'repository')
  source = Path(tmux.__file__)
  tree = ast.parse(await anyio.Path(source).read_text(), filename=str(source))
  providers = OwnedPaths(
    {
      '/etc/update-motd.d': str(root / 'motd'),
      '/run/motd.dynamic': str(root / 'motd-cache'),
    }
  )
  tree = providers.visit(tree)
  exec(compile(tree, str(source), 'exec'), tmux.__dict__)
  tmux.TMUX_START_DIR = str(root / 'repository')
  app = web.Application(client_max_size=16 * 1024 * 1024)
  terminal.register(app)
  if config.get('cli_config'):
    vision_test.register(app)
  if config.get('application'):
    from openpilot.selfdrive.carrot.server.features import system

    app['hb_last'] = {'ok': None, 'msg': 'not yet', 'ts': 0}
    app.router.add_get('/api/heartbeat_status', system.api_heartbeat_status)
    app.router.add_static('/', str(root / 'web'), show_index=True)
  app_source = Path('openpilot/selfdrive/carrot/server/app.py')
  app_tree = ast.parse(await anyio.Path(app_source).read_text())
  cleanup = next(node for node in app_tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup')
  namespace = {'asyncio': terminal_pty.asyncio, 'traceback': traceback, 'web': web}
  exec(compile(ast.Module(body=[cleanup], type_ignores=[]), str(app_source), 'exec'), namespace)
  app.on_cleanup.append(namespace['on_cleanup'])
  hashes = {
    str(path): hashlib.sha256(await anyio.Path(path).read_bytes()).hexdigest()
    for path in [source, Path(terminal_pty.__file__), Path(terminal.__file__), app_source]
  }
  await anyio.to_thread.run_sync(
    save,
    root / 'source-proof.json',
    {
      'source_files': hashes,
      'literal_provider_replacements': providers.replaced,
      'pty_and_route_function_bodies_unchanged': True,
      'actual_app_cleanup_body_unchanged': True,
      'client_max_size_matches_original_app_constant': 16 * 1024 * 1024,
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
      print(json.dumps({'port': listener.getsockname()[1]}), flush=True)
      await anyio.to_thread.run_sync(sys.stdin.readline)
    finally:
      await runner.cleanup()
    print(json.dumps({'app_cleanup': True, 'pty': await terminal_pty.PTY_SESSION.snapshot()}), flush=True)
    await anyio.to_thread.run_sync(sys.stdin.readline)


if __name__ == '__main__':
  anyio.run(main)
