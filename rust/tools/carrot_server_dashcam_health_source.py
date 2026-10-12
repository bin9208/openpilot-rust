# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Started by carrot_server_dashcam_health.py with source dependencies supplied by the caller.
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
from types import SimpleNamespace
from typing import Any

import anyio
from aiohttp import web
from check_dashcam_runtime import module


def application(config: dict):
    from openpilot.selfdrive.carrot.server.services import web_settings

    transport_path = Path('openpilot/selfdrive/carrot/web_upload.py')
    transport = module(transport_path, 'health_source_transport')
    upload_path = Path('openpilot/selfdrive/carrot/server/features/dashcam/upload.py')
    upload_text = upload_path.read_text()
    names = {'param_text', 'repo_dir', 'git_text', 'device_serial', 'upload_metadata', 'current_upload_metadata', 'upload_target_settings'}
    nodes = [node for node in ast.parse(upload_text).body if isinstance(node, ast.FunctionDef) and node.name in names]
    upload_scope = {'Any': Any, 'os': os, 'subprocess': subprocess, 'HAS_PARAMS': False,
                    'HARDWARE': SimpleNamespace(get_serial=lambda: ''),
                    'read_web_settings': web_settings.read_web_settings,
                    'web_upload_settings': transport.web_upload_settings}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(upload_path), 'exec'), upload_scope)
    route_path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    route_text = route_path.read_text()
    route = next(node for node in ast.parse(route_text).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'api_dashcam_upload_test')
    scope = {'web': web, 'upload': SimpleNamespace(**upload_scope),
             'check_web_upload_health': transport.check_web_upload_health,
             'create_web_upload_session': transport.create_web_upload_session}
    exec(compile(ast.Module(body=[route], type_ignores=[]), str(route_path), 'exec'), scope)
    app_path = Path('openpilot/selfdrive/carrot/server/app.py')
    app_text = app_path.read_text()
    limit_assignment = next(node for node in ast.parse(app_text).body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'VISION_DIAG_UPLOAD_MAX_BYTES' for target in node.targets))
    limit_scope = {}
    exec(compile(ast.Module(body=[limit_assignment], type_ignores=[]), str(app_path), 'exec'), limit_scope)
    app = web.Application(client_max_size=limit_scope['VISION_DIAG_UPLOAD_MAX_BYTES'])
    app.router.add_post('/api/dashcam/upload/test', scope['api_dashcam_upload_test'])
    proof = {'route': {'path': str(route_path), 'line': route.lineno, 'sha256': hashlib.sha256(ast.get_source_segment(route_text, route).encode()).hexdigest()},
             'metadata_functions': [{'name': node.name, 'line': node.lineno, 'sha256': hashlib.sha256(ast.get_source_segment(upload_text, node).encode()).hexdigest()} for node in nodes],
             'body_limit': {'value': limit_scope['VISION_DIAG_UPLOAD_MAX_BYTES'], 'assignment_sha256': hashlib.sha256(ast.get_source_segment(app_text, limit_assignment).encode()).hexdigest(), 'reuse': 'same original16MiB constant/shared decoder proof; current health requests have empty bodies'},
             'source_files': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (route_path, upload_path, transport_path, Path(web_settings.__file__))},
             'providers': {'Params': 'HAS_PARAMS=False; native Params=None', 'metadata': 'actual source helpers and owned Git repository', 'hardware_serial': 'empty PC provider; owned CARROT_DEVICE_SERIAL wins', 'settings': 'actual read_web_settings on owned CARROT_DATA_DIR'}}
    (Path(config['output'])/'source-proof.json').write_text(json.dumps(proof, indent=2)+'\n')
    return app


async def main() -> None:
    config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
    runner = web.AppRunner(application(config))
    listener = socket.socket()
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
        listener.close()


if __name__ == '__main__':
    anyio.run(main)
