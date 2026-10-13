#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import traceback

import anyio
from aiohttp import ClientSession, web
from carrot_server_dashcam_upload import save
from carrot_server_terminal_command_source import configure
from carrot_server_tools_source import OwnedPaths


async def main() -> None:
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  root = Path(str(await anyio.Path(config['owned_root']).resolve()))
  assert root.is_relative_to(Path.cwd())
  configure(Path(config['cli_config']))
  os.environ.update(json.loads(await anyio.Path(root / 'support-environment.json').read_text()))
  from openpilot.selfdrive.carrot.server.features import support_terminal, terminal
  from openpilot.selfdrive.carrot.server.services import support_terminal as support, terminal_pty, tmux

  source = Path(tmux.__file__)
  providers = OwnedPaths({'/etc/update-motd.d': str(root / 'motd'), '/run/motd.dynamic': str(root / 'motd-cache')})
  tree = providers.visit(ast.parse(await anyio.Path(source).read_text(), filename=str(source)))
  exec(compile(tree, str(source), 'exec'), tmux.__dict__)
  tmux.TMUX_START_DIR = str(root / 'repository')
  app = web.Application(client_max_size=16 * 1024 * 1024)
  app['http'] = ClientSession()
  terminal.register(app)
  support_terminal.register(app)
  stream_target = root / 'stream-target.json'
  if stream_target.is_file():
    from openpilot.selfdrive.carrot.server.features import stream
    from openpilot.selfdrive.carrot.server.services.params import Params
    stream.WEBRTCD_URL = json.loads(stream_target.read_text())['target']
    app['params'] = Params()
    stream.register(app)
  app_source = Path('openpilot/selfdrive/carrot/server/app.py')
  cleanup = next(
    node for node in ast.parse(await anyio.Path(app_source).read_text()).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup'
  )
  namespace = {'asyncio': terminal_pty.asyncio, 'traceback': traceback, 'web': web}
  exec(compile(ast.Module(body=[cleanup], type_ignores=[]), str(app_source), 'exec'), namespace)
  app.on_cleanup.append(namespace['on_cleanup'])
  await anyio.to_thread.run_sync(
    save,
    root / 'support-source-proof.json',
    {
      'source_files': {
        str(path): hashlib.sha256(await anyio.Path(path).read_bytes()).hexdigest()
        for path in [
          Path(support_terminal.__file__),
          Path(support.__file__),
          Path(terminal_pty.__file__),
          Path('openpilot/selfdrive/carrot/server/services/support_tunnel.py'),
          Path('openpilot/selfdrive/carrot/server/services/support_discord.py'),
          Path('openpilot/selfdrive/carrot/server/features/stream.py'),
          Path('openpilot/selfdrive/carrot/server/services/vision_diag.py'),
          app_source,
        ]
      },
      'support_function_bodies_unchanged': True,
      'actual_app_cleanup_body_unchanged': True,
      'owned_motd_literals': providers.replaced,
      'same_pty_object': terminal.PTY_SESSION is support.PTY_SESSION,
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
    if stream_target.is_file():
      from openpilot.selfdrive.carrot.server.services.vision_diag import get_stream_proxy_history
      await anyio.to_thread.run_sync(save, root / 'stream-history.json', get_stream_proxy_history())
    session = support.manager.current()
    child = session.tunnel.process if session and session.tunnel else None
    print(
      json.dumps(
        {
          'app_cleanup': True,
          'pty': await terminal_pty.PTY_SESSION.snapshot(),
          'support': support.manager.snapshot(),
          'guest_origin_alive': session is not None and session.local_runner is not None,
          'tunnel_child_alive': child is not None and child.returncode is None,
        }
      ),
      flush=True,
    )
    await anyio.to_thread.run_sync(sys.stdin.readline)
    if session:
      await support.manager.stop('fixture')
    if child:
      with anyio.fail_after(6):
        await child.wait()


if __name__ == '__main__':
  anyio.run(main)
