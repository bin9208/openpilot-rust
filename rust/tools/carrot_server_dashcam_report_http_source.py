# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Launched by the report HTTP oracle with caller-provided original dependencies.
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
from carrot_server_dashcam_catalog import source_modules


async def main() -> None:
    data = json.loads(await anyio.to_thread.run_sync(sys.stdin.readline))
    catalog, paths, _ = source_modules()
    from openpilot.selfdrive.carrot.server.features.dashcam import report
    catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = data['root']
    path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    text = path.read_text()
    node = next(node for node in ast.parse(text).body if isinstance(node, ast.AsyncFunctionDef) and node.name == 'api_dashcam_report')
    scope = {'web': web, 'asyncio': sys.modules['asyncio'], 'build_route_report': report.build_route_report}
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), 'exec'), scope)
    app_path = Path('openpilot/selfdrive/carrot/server/app.py')
    limit = next(node for node in ast.parse(app_path.read_text()).body if isinstance(node, ast.Assign)
        and any(isinstance(target, ast.Name) and target.id == 'VISION_DIAG_UPLOAD_MAX_BYTES' for target in node.targets))
    exec(compile(ast.Module(body=[limit], type_ignores=[]), str(app_path), 'exec'), scope)
    app = web.Application(client_max_size=scope['VISION_DIAG_UPLOAD_MAX_BYTES'])
    app.router.add_get('/api/dashcam/report/{route}', scope['api_dashcam_report'])
    output = Path(data['output'])
    (output/'source-proof.json').write_text(json.dumps({'api_sha256': hashlib.sha256(ast.get_source_segment(text,node).encode()).hexdigest(),
        'report_sha256': hashlib.sha256(Path(report.__file__).read_bytes()).hexdigest(),
        'owned_root': data['root'], 'body_limit': scope['VISION_DIAG_UPLOAD_MAX_BYTES']}, indent=2)+'\n')
    runner = web.AppRunner(app)
    with socket.socket() as listener:
        try:
            await runner.setup()
            listener.bind(('127.0.0.1',0)); listener.listen(); listener.setblocking(False)
            await web.SockSite(runner,listener).start()
            print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}),flush=True)
            await anyio.to_thread.run_sync(sys.stdin.readline)
        finally:
            await runner.cleanup()


if __name__ == '__main__':
    anyio.run(main)
