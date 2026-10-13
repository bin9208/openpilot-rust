# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Launched by the sync HTTP oracle with caller-supplied source dependencies.
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import socket
import sys

import anyio
from aiohttp import web
from carrot_server_dashcam_sync_source import application


async def main() -> None:
    config = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
    output = Path(config['output'])
    app, scope = application(config)
    path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    text = path.read_text()
    names = {'api_dashcam_upload_start', 'api_dashcam_upload_job', 'api_dashcam_upload_cancel'}
    nodes = [node for node in ast.parse(text).body if isinstance(node, ast.AsyncFunctionDef) and node.name in names]
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
    for operation in ('start', 'cancel'):
        app.router.add_post('/api/dashcam/upload/'+operation, scope['api_dashcam_upload_'+operation])
    app.router.add_get('/api/dashcam/upload/job', scope['api_dashcam_upload_job'])
    proof = json.loads((output/'source-proof.json').read_text())
    proof['async_route_functions'] = [{'name': node.name, 'line': node.lineno,
        'sha256': hashlib.sha256(ast.get_source_segment(text, node).encode()).hexdigest()} for node in nodes]
    (output/'source-proof.json').write_text(json.dumps(proof, indent=2)+'\n')
    runner = web.AppRunner(app)
    with socket.socket() as listener:
        try:
            await runner.setup()
            listener.bind(('127.0.0.1', 0)); listener.listen(); listener.setblocking(False)
            await web.SockSite(runner, listener).start()
            print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}), flush=True)
            while line := await anyio.to_thread.run_sync(sys.stdin.readline):
                if not line.strip(): break
                command = json.loads(line)
                assert command['op'] == 'jobs'
                print(json.dumps({'jobs': list(scope['jobs']())}), flush=True)
        finally:
            await runner.cleanup()
            (output/'jobs-at-exit.json').write_text(json.dumps(list(scope['jobs']()))+'\n')


if __name__ == '__main__':
    anyio.run(main)
