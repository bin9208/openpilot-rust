# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp"]
# ///
# Invoked by the paired HTTP/lifecycle driver with an owned line-JSON configuration on stdin.
from __future__ import annotations

import ast
import asyncio
import hashlib
import json
from pathlib import Path
import sys
from types import ModuleType

from aiohttp import web
from carrot_server_git_state_source import original as original_state
from carrot_server_git_status_source import original as original_status


def original_routes(status: ModuleType, state: ModuleType, root: Path) -> ModuleType:
  source = Path(__file__).resolve().parents[2] / 'openpilot/selfdrive/carrot/server'
  tree = ast.parse((source / 'features/tools/routes.py').read_text())
  route = next(node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'api_tools_git_status')
  module = ModuleType('owned_original_git_routes')
  module.web = web
  module.asyncio = asyncio
  module.get_git_status = status.get_git_status
  module.read_auto_update_state = state.read_auto_update_state
  module.git_status_loop = status.git_status_loop
  exec(compile(ast.Module(body=[route], type_ignores=[]), str(source / 'features/tools/routes.py'), 'exec'), module.__dict__)
  app_tree = ast.parse((source / 'app.py').read_text())
  startup = next(node for node in app_tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_startup')
  cleanup = next(node for node in app_tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup')
  assignment = next(node for node in startup.body if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == "app['git_status_task']")
  begin = next(index for index, node in enumerate(cleanup.body) if isinstance(node, ast.Assign) and ast.unparse(node.targets[0]) == 'git_status_task')
  original_nodes = [assignment, *cleanup.body[begin:begin + 2]]
  startup.body = [assignment]
  cleanup.body = cleanup.body[begin:begin + 2]
  exec(compile(ast.Module(body=[startup, cleanup], type_ignores=[]), str(source / 'app.py'), 'exec'), module.__dict__)
  hashes = {'api_tools_git_status': hashlib.sha256(ast.dump(route).encode()).hexdigest(),
            'git_lifecycle_nodes': [hashlib.sha256(ast.dump(node).encode()).hexdigest() for node in original_nodes]}
  (root / 'original-body-hashes.json').write_text(json.dumps(hashes, indent=2))
  return module


async def main() -> None:
  config = json.loads(sys.stdin.readline())
  root = Path(config['root'])
  status = original_status()
  status.REPO_DIR = config['repo']
  status._now = lambda: 1000.0
  from openpilot.common import repo_update
  repo_update.LOCK_PATH = config['lock']
  state = original_state(root / 'data/state')
  routes = original_routes(status, state, root)
  app = web.Application()
  app.router.add_get('/api/tools/git_status', routes.api_tools_git_status)
  app.router.add_get('/plain.txt', lambda _: web.FileResponse(root / 'web/plain.txt'))
  if config.get('composed'):
    app.on_startup.append(routes.on_startup)
    app.on_cleanup.append(routes.on_cleanup)
  runner = web.AppRunner(app)
  try:
    await runner.setup()
    site = web.TCPSite(runner, '127.0.0.1', 0)
    await site.start()
    print(json.dumps({'port': site._server.sockets[0].getsockname()[1]}), flush=True)
    command = await asyncio.to_thread(sys.stdin.readline)
    if command.strip() != 'stop':
      raise RuntimeError('unknown owned source stop command')
  finally:
    await runner.cleanup()
  print(json.dumps({'serve_error': None}), flush=True)


if __name__ == '__main__':
  asyncio.run(main())
