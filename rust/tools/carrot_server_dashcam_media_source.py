from __future__ import annotations

import ast
import asyncio
import hashlib
import os
from pathlib import Path
from types import ModuleType

from aiohttp import web
from carrot_server_dashcam_catalog import source_modules

FUNCTIONS = frozenset(('api_dashcam_thumbnail', 'api_dashcam_preview', 'api_dashcam_video'))

def source(root: Path, cache: Path):
  catalog, paths, _ = source_modules()
  catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = str(root)
  paths.DASHCAM_CACHE_DIR = str(cache)
  from openpilot.selfdrive.carrot.server.features.dashcam import ffmpeg
  path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
  text = path.read_text(); tree = ast.parse(text, filename=str(path))
  nodes = [node for node in tree.body if isinstance(node, ast.AsyncFunctionDef) and node.name in FUNCTIONS]
  routes = ModuleType('original_dashcam_media_routes')
  routes.__dict__.update(asyncio=asyncio, os=os, web=web,
    ensure_thumbnail=ffmpeg.ensure_thumbnail, ensure_preview=ffmpeg.ensure_preview, browser_video=ffmpeg.browser_video)
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), routes.__dict__)
  ffmpeg_path = Path(ffmpeg.__file__); ffmpeg_text = ffmpeg_path.read_text()
  ffmpeg_nodes = [node for node in ast.parse(ffmpeg_text).body if isinstance(node, ast.FunctionDef)]
  proof = {'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
    'ffmpeg_sha256': hashlib.sha256(ffmpeg_path.read_bytes()).hexdigest(),
    'functions': [{'name': node.name, 'line': node.lineno,
      'sha256': hashlib.sha256(ast.get_source_segment(text,node).encode()).hexdigest()} for node in nodes],
    'ffmpeg_functions': [{'name': node.name, 'line': node.lineno,
      'sha256': hashlib.sha256(ast.get_source_segment(ffmpeg_text,node).encode()).hexdigest()} for node in ffmpeg_nodes],
    'provider_remapping': {'DASHCAM_ROOT': str(root), 'DASHCAM_CACHE_DIR': str(cache), 'PATH': os.environ['PATH']},
    'method': 'unchanged original three route AST bodies plus unchanged original ffmpeg.py import; only owned root/cache/PATH remapping'}
  return routes, ffmpeg, proof

def application(routes: ModuleType) -> web.Application:
  app = web.Application()
  for kind in ('thumbnail', 'preview', 'video'):
    app.router.add_get('/api/dashcam/'+kind+'/{segment}',getattr(routes,'api_dashcam_'+kind))
  return app
