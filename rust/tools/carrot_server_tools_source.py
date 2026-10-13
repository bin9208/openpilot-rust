#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Actual Tools routes/jobs/dispatcher with only fixed provider paths bound to an owned root."""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import resource
import socket
import sys

import anyio
from aiohttp import web
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_upload import save
from original_params_binding import load


class OwnedPaths(ast.NodeTransformer):
  def __init__(self, paths: dict[str, str]) -> None:
    self.paths = paths
    self.replaced: list[tuple[str, str]] = []

  def visit_Constant(self, node: ast.Constant) -> ast.Constant:
    if isinstance(node.value, str):
      original = node.value
      value = original
      for source, target in self.paths.items():
        value = value.replace(source, target)
      if value != original:
        self.replaced.append((original, value))
        return ast.copy_location(ast.Constant(value), node)
    return node


async def main() -> None:
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  root = Path(str(await anyio.Path(config['owned_root']).resolve()))
  assert root != Path('/') and root.is_relative_to(Path.cwd())
  assert Path(os.environ['PARAMS_ROOT']).is_relative_to(root)
  source_modules()
  binding = os.environ['ORIGINAL_PARAMS_BINDING']
  load(binding, f'ipc://{root}/owned-log', root / 'logs')
  from openpilot.common import repo_update
  from openpilot.selfdrive.carrot.server.features.tools import actions, dispatcher, jobs, routes
  from openpilot.selfdrive.carrot.server.services import git_status

  repo_update.LOCK_PATH = str(root / 'repository.lock')
  git_status.REPO_DIR = str(root / 'repository')
  jobs.CARROT_STATE_DIR = str(root / 'state')
  jobs.CARROT_TOOL_JOBS_STATE_PATH = str(root / 'state/tool_jobs.json')
  paths = {
    '/data/openpilot': str(root / 'repository'),
    '/data/media': str(root / 'media'),
    '/data/params': str(root / 'owned-params'),
  }
  dispatcher.TMUX_LOG_PATH = str(root / 'media/tmux.log')
  dispatcher.PARAMS_BACKUP_PATH = str(root / 'media/params_backup.json')
  source = Path(dispatcher.__file__)
  tree = ast.parse(await anyio.Path(source).read_text(), filename=str(source))
  providers = OwnedPaths(paths)
  tree = providers.visit(tree)
  # Function statements and call structure stay original; only fixed provider path literals change.
  exec(compile(tree, str(source), 'exec'), dispatcher.__dict__)
  dispatcher.TMUX_LOG_PATH = str(root / 'media/tmux.log')
  dispatcher.PARAMS_BACKUP_PATH = str(root / 'media/params_backup.json')
  routes.dispatch_sync = dispatcher.dispatch_sync
  routes.run_tool_job = dispatcher.run_tool_job
  hashes = {}
  for module in [actions, dispatcher, jobs, routes]:
    hashes[module.__file__] = hashlib.sha256(await anyio.Path(module.__file__).read_bytes()).hexdigest()
  if config.get('application'):
    from openpilot.selfdrive.carrot.server import config as original_config

    original_config.CARROT_LEGACY_STATE_DIR = str(root / 'legacy-state')
    original_config.migrate_legacy_carrot_state()
  app = web.Application(client_max_size=16 * 1024 * 1024)
  routes.register(app)
  if config.get('application'):
    from openpilot.selfdrive.carrot.server.features import params as params_routes, system

    params_routes.PARAMS_BACKUP_PATH = str(root / 'media/params_backup.json')
    app['hb_last'] = {'ok': None, 'msg': 'not yet', 'ts': 0}
    app.router.add_get('/api/heartbeat_status', system.api_heartbeat_status)
    params_routes.register(app)
    app.router.add_static('/', str(root / 'web'), show_index=True)
  await anyio.to_thread.run_sync(
    save,
    root / 'source-proof.json',
    {
      'source_files': hashes,
      'provider_path_literals': providers.replaced,
      'actual_cython_params': binding,
      'actual_tools_functions_and_process_groups': True,
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
      await anyio.to_thread.run_sync(save, root / 'app-cleanup.json', {'jobs': jobs.list_snapshots(), 'app_runner_cleanup_finished': True})


if __name__ == '__main__':
  anyio.run(main)
