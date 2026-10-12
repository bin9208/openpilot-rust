from __future__ import annotations

import ast
import asyncio
import hashlib
import os
from pathlib import Path
import threading
from types import ModuleType, SimpleNamespace
from typing import Any

from aiohttp import web
from carrot_server_dashcam_catalog import source_modules

FUNCTIONS = frozenset((
  '_realdata_signature', 'cached_dashcam_routes', '_newest_first_segments',
  'visible_dashcam_routes', 'recent_completed_dashcam_segments', 'bounded_query_int',
  'normalized_sort', 'route_with_segment_page', 'find_dashcam_route',
  '_routes_page_payload', 'api_dashcam_routes', '_segments_page_payload',
  'api_dashcam_segments', 'api_dashcam_recent_segments', 'api_dashcam_read_state',
  'api_dashcam_read_state_update',
))
GLOBALS = frozenset(('ROUTE_CACHE_MAX_AGE', 'DASHCAM_ROUTE_LIMIT_DEFAULT',
  'DASHCAM_ROUTE_LIMIT_MAX', 'DASHCAM_SEGMENT_LIMIT_DEFAULT', 'DASHCAM_SEGMENT_LIMIT_MAX',
  'DASHCAM_OFFSET_MAX', 'DASHCAM_RECENT_UPLOAD_LIMITS', '_route_cache_lock', '_route_cache'))

def selected_source(root: Path, state_path: Path, clock: list[float], wall: int) -> tuple[ModuleType, dict[str, Any]]:
  catalog, paths, state = source_modules()
  catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = str(root)
  catalog._end_epoch_cache = {}
  paths.time = state.time = SimpleNamespace(time=lambda: wall)
  state.CARROT_DASHCAM_READ_STATE_PATH = str(state_path)
  path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
  source = path.read_text(); tree = ast.parse(source, filename=str(path))
  nodes = []
  for node in tree.body:
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in FUNCTIONS:
      nodes.append(node)
    elif isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id in GLOBALS for target in node.targets):
      nodes.append(node)
    elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id in GLOBALS:
      nodes.append(node)
  selected = ast.Module(body=nodes, type_ignores=[])
  module = ModuleType('original_dashcam_catalog_http')
  module.__dict__.update(asyncio=asyncio, os=os, threading=threading,
    time=SimpleNamespace(monotonic=lambda: clock[0]), web=web, DASHCAM_ROOT=str(root),
    build_routes=catalog.build_routes, compute_segment_times=catalog.compute_segment_times,
    invalidate_segment_time_cache=catalog.invalidate_segment_time_cache,
    route_time_bounds=catalog.route_time_bounds, segment_is_complete=catalog.segment_is_complete,
    relative_time=paths.relative_time, read_dashcam_read_state=state.read_dashcam_read_state,
    write_dashcam_recent_segment=state.write_dashcam_recent_segment)
  exec(compile(selected, str(path), 'exec'), module.__dict__)
  provenance = {'source_path': str(path), 'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
    'functions': [{'name': node.name, 'line': node.lineno,
      'source_segment_sha256': hashlib.sha256(ast.get_source_segment(source, node).encode()).hexdigest()}
      for node in nodes if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))],
    'method': 'compile original AST nodes without body edits; only selected globals/dependencies installed; excluded report/media/upload functions are never imported or called'}
  return module, provenance

def application(source: ModuleType, clock: list[float]) -> web.Application:
  app = web.Application(client_max_size=16 * 1024 * 1024)
  for path, name in (('routes', 'api_dashcam_routes'), ('segments/{route}', 'api_dashcam_segments'),
      ('recent', 'api_dashcam_recent_segments'), ('read-state', 'api_dashcam_read_state')):
    app.router.add_get('/api/dashcam/' + path, getattr(source, name))
  app.router.add_post('/api/dashcam/read-state', source.api_dashcam_read_state_update)
  async def control(request: web.Request) -> web.Response:
    value = await request.json()
    try:
      match value['operation']:
        case 'clock': clock[0] = value['now']; result = None
        case 'cached': result = await asyncio.to_thread(source.cached_dashcam_routes)
        case 'bounds': result = list(await asyncio.to_thread(source.route_time_bounds, value['segments']))
        case _: raise RuntimeError('unknown fixture control')
      return web.json_response({'value': result})
    except Exception as error:
      return web.json_response({'error': str(error)})
  app.router.add_post('/__fixture/control', control)
  return app
