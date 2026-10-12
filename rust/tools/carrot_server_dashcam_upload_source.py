from __future__ import annotations

import ast
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

from aiohttp import web
from check_dashcam_runtime import source_scope

FUNCTIONS = frozenset(('request_upload_segments', 'api_dashcam_upload_summary', 'api_dashcam_upload_start', 'api_dashcam_upload_job', 'api_dashcam_upload_cancel'))

def application(root: Path, settings: dict):
    scope, config = source_scope()
    scope['DASHCAM_ROOT'] = str(root)
    config.update(settings)
    os.environ['CARROT_WEB_UPLOAD_CONCURRENCY'] = str(settings['concurrency'])
    scope['upload_jobs'] = SimpleNamespace(**scope)
    path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
    text = path.read_text()
    tree = ast.parse(text, filename=str(path))
    nodes = [node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name in FUNCTIONS]
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), scope)
    app_path = Path('openpilot/selfdrive/carrot/server/app.py')
    app_text = app_path.read_text()
    assignment = next(node for node in ast.parse(app_text).body if isinstance(node,ast.Assign) and any(isinstance(target,ast.Name) and target.id=='VISION_DIAG_UPLOAD_MAX_BYTES' for target in node.targets))
    limit = {}
    exec(compile(ast.Module(body=[assignment],type_ignores=[]),str(app_path),'exec'),limit)
    app = web.Application(client_max_size=limit['VISION_DIAG_UPLOAD_MAX_BYTES'])
    for operation in ('summary', 'start', 'cancel'):
        app.router.add_post('/api/dashcam/upload/'+operation, scope['api_dashcam_upload_'+operation])
    app.router.add_get('/api/dashcam/upload/job', scope['api_dashcam_upload_job'])
    proof = {
        'source_routes_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'body_limit': {'value':limit['VISION_DIAG_UPLOAD_MAX_BYTES'],'source':str(app_path),'assignment_sha256':hashlib.sha256(ast.get_source_segment(app_text,assignment).encode()).hexdigest(),'reuse':'unchanged shared request decoder proof; no oversized corpus replay'},
        'source_functions': [{'name': node.name, 'line': node.lineno, 'sha256': hashlib.sha256(ast.get_source_segment(text, node).encode()).hexdigest()} for node in nodes],
        'unchanged_library_source': {
            str(path): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in [
                *[Path('openpilot/selfdrive/carrot/server/features/dashcam')/name for name in ('upload_jobs.py','catalog.py','paths.py')],
                Path('openpilot/selfdrive/carrot/web_upload.py'),
                Path('openpilot/selfdrive/carrot/server/services/dashcam_upload_report.py'),
            ]
        },
        'providers': {'root': str(root), 'settings': settings, 'Params': 'HAS_PARAMS=False; same explicit settings sent to native worker', 'report_commit_prefix': 'existing check_dashcam_runtime source mapper changes ajouatom/openpilot to bin9208/openpilot-rust'},
    }
    return app, scope, proof

async def main() -> None:
    config = json.loads(await asyncio.to_thread(sys.stdin.readline))
    output = Path(config['output'])
    app, scope, proof = application(Path(config['root']), config['settings'])
    (output/'source-proof.json').write_text(json.dumps(proof, indent=2)+'\n')
    runner = web.AppRunner(app)
    await runner.setup()
    import socket
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen()
    listener.setblocking(False)
    await web.SockSite(runner, listener).start()
    print(json.dumps({'port': listener.getsockname()[1], 'pid': os.getpid()}), flush=True)
    try:
        await asyncio.to_thread(sys.stdin.readline)
    finally:
        await runner.cleanup()
        (output/'source-jobs-before-loop-shutdown.json').write_text(json.dumps([scope['snapshot'](job) for job in scope['jobs']().values()], indent=2)+'\n')

if __name__ == '__main__':
    asyncio.run(main())
