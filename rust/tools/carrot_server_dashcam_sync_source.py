# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Launched by the sync probe with caller-supplied source dependencies.
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time
import traceback
from types import SimpleNamespace

import anyio
from aiohttp import web
from check_dashcam_metadata import source as metadata_source
from check_dashcam_runtime import source_scope


class FileParams:
    def __init__(self, directory: Path):
        self.directory = directory

    def get(self, key: str) -> bytes:
        if key not in {'CarName', 'DongleId'}:
            raise KeyError(key)
        path = self.directory/key
        return path.read_bytes() if path.is_file() else b''


def application(config: dict):
    output = Path(config['output'])
    scope, _settings = source_scope()
    scope['DASHCAM_ROOT'] = config['root']
    scope['HAS_PARAMS'] = True
    directory = Path(os.environ['PARAMS_ROOT'])/os.environ['OPENPILOT_PREFIX']
    scope['Params'] = lambda: FileParams(directory)
    actual = metadata_source(Path.cwd(), Path(config['state'])/'state/web_settings.json')
    actual['HARDWARE'] = SimpleNamespace(get_serial=lambda: '')
    previous = scope['upload']
    def observed_webhook(params):
        value = actual['discord_webhook_url'](params)
        (output/'webhook-lookup.json').write_text(json.dumps({'url': value, 'monotonic': time.monotonic()})+'\n')
        return value
    scope['upload'] = SimpleNamespace(upload_target_settings=actual['upload_target_settings'],
        upload_metadata=actual['upload_metadata'], upload_share_text=previous.upload_share_text,
        discord_webhook_url=observed_webhook, send_discord_webhook=previous.send_discord_webhook)
    scope['upload_jobs'] = SimpleNamespace(**scope)
    path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    text = path.read_text()
    nodes = [node for node in ast.parse(text).body if isinstance(node, ast.AsyncFunctionDef) and node.name in {'request_upload_segments', 'api_dashcam_upload'}]
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
    app_path = Path('openpilot/selfdrive/carrot/server/app.py')
    app_text = app_path.read_text()
    tree = ast.parse(app_text)
    limit = next(node for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'VISION_DIAG_UPLOAD_MAX_BYTES' for target in node.targets))
    cleanup = next(node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'on_cleanup')
    app_scope = {'web': web, 'asyncio': scope['asyncio'], 'traceback': traceback}
    exec(compile(ast.Module(body=[limit, cleanup], type_ignores=[]), str(app_path), 'exec'), app_scope)
    app = web.Application(client_max_size=app_scope['VISION_DIAG_UPLOAD_MAX_BYTES'])
    app.on_cleanup.append(app_scope['on_cleanup'])

    cancelled_error = scope['asyncio'].CancelledError
    async def observed(request):
        try:
            response = await scope['api_dashcam_upload'](request)
            (output/'handler-return.json').write_text(json.dumps({'status': response.status, 'payload': json.loads(response.body)}, indent=2)+'\n')
            return response
        except cancelled_error:
            (output/'handler-cancelled.json').write_text('{"cancelled":true}\n')
            raise

    app.router.add_post('/api/dashcam/upload', observed)
    proof = {'functions': [{'name': node.name, 'line': node.lineno, 'sha256': hashlib.sha256(ast.get_source_segment(text, node).encode()).hexdigest()} for node in nodes],
             'source_files': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [path, app_path, Path('openpilot/selfdrive/carrot/server/features/dashcam/upload_jobs.py'), Path('openpilot/selfdrive/carrot/server/features/dashcam/upload.py')]},
             'cleanup_sha256': hashlib.sha256(ast.get_source_segment(app_text, cleanup).encode()).hexdigest(),
             'providers': {'Params': 'owned FileParams; CarName/DongleId bytes, remaining probed keys unknown, matching current native Params schema', 'metadata': 'actual source helpers; owned Params root/Git repo/serial env', 'webhook': 'unchanged named source os.environ getter; immutable owned loopback URL; wrapper observes invocation time only'},
             'report_mapping': 'unchanged check_dashcam_runtime source mapper maps source commit links to bin9208/openpilot-rust',
             'body_limit': app_scope['VISION_DIAG_UPLOAD_MAX_BYTES']}
    (output/'source-proof.json').write_text(json.dumps(proof, indent=2)+'\n')
    return app, scope


async def main() -> None:
    config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
    output = Path(config['output'])
    app, scope = application(config)
    runner = web.AppRunner(app)
    listener = socket.socket()
    try:
        await runner.setup()
        listener.bind(('127.0.0.1', 0)); listener.listen(); listener.setblocking(False)
        await web.SockSite(runner, listener).start()
        print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}), flush=True)
        while line := await anyio.to_thread.run_sync(sys.stdin.readline):
            if not line.strip():
                break
            command = json.loads(line)
            if command['op'] == 'jobs':
                print(json.dumps({'jobs': list(scope['jobs']())}), flush=True)
        await runner.cleanup()
    finally:
        await runner.cleanup()
        listener.close()
        (output/'jobs-at-exit.json').write_text(json.dumps(list(scope['jobs']()))+'\n')


if __name__ == '__main__':
    anyio.run(main)
