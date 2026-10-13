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

import anyio
from aiohttp import ClientSession, web
from carrot_server_dashcam_upload import save
from carrot_server_terminal_command_source import configure
from carrot_server_tools_source import OwnedPaths


async def main() -> None:
  input = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
  root = Path(input['owned_root']).resolve()
  assert root.is_relative_to(Path.cwd()) and root != Path.cwd()
  _, vision = configure(Path(input['cli_config']))
  os.environ.update(json.loads(await anyio.Path(root / 'support-environment.json').read_text()))
  from openpilot.selfdrive.carrot.server.features import vision_diag as feature
  from openpilot.selfdrive.carrot.server.services import vision_diag as service

  source = Path(service.__file__)
  owned = json.loads(await anyio.Path(root / 'diagnostic-config.json').read_text())
  bindings = OwnedPaths({'/proc': owned['proc_root']})
  tree = bindings.visit(ast.parse(await anyio.Path(source).read_text()))
  functions = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))]
  for node in ast.walk(ast.Module(body=functions, type_ignores=[])):
    if isinstance(node, ast.Constant) and node.value == 5001:
      node.value = owned['port']
  exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), service.__dict__)
  service.VISION_TEST_LOG_PATH = vision.LOG_PATH
  service._PROCESS_MATCHES = dict(owned['process_matches'])
  feature.get_server_diagnostic_snapshot = service.get_server_diagnostic_snapshot
  feature.upload_diagnostic_bundle_to_discord = service.upload_diagnostic_bundle_to_discord
  app = web.Application(client_max_size=16 * 1024 * 1024)
  feature.register(app)
  if (root / 'stream-target.json').is_file():
    from openpilot.selfdrive.carrot.server.features import stream
    from openpilot.selfdrive.carrot.server.services.params import Params
    stream.WEBRTCD_URL = json.loads(await anyio.Path(root / 'stream-target.json').read_text())['target']
    app['http'] = ClientSession()
    app['params'] = Params()
    stream.register(app)
  runner = web.AppRunner(app)
  await anyio.to_thread.run_sync(save, root / 'diagnostic-source-proof.json', {
    'source_files': {str(path): hashlib.sha256(await anyio.Path(path).read_bytes()).hexdigest() for path in [source, Path(feature.__file__)]},
    'function_bodies_preserved_except_owned_proc_and_fixed_port': True,
    'owned_proc_bindings': bindings.replaced,
    'fixed_port_binding': owned['port'],
    'process_pattern_bindings': owned['process_matches'],
    'feature_calls_bound_service_functions': True,
  })
  with socket.socket() as listener:
    await runner.setup()
    listener.bind(('127.0.0.1', 0))
    listener.listen()
    listener.setblocking(False)
    await web.SockSite(runner, listener).start()
    print(json.dumps({'port': listener.getsockname()[1]}), flush=True)
    await anyio.to_thread.run_sync(sys.stdin.readline)
    await runner.cleanup()
    if 'http' in app:
      await app['http'].close()
    print(json.dumps({'app_cleanup': True}), flush=True)
    await anyio.to_thread.run_sync(sys.stdin.readline)


if __name__ == '__main__':
  anyio.run(main)
