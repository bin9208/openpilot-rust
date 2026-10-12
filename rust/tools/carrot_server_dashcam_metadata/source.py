from __future__ import annotations

import ast
import hashlib
import mimetypes
import os
from pathlib import Path
from types import ModuleType
from urllib.parse import quote

from aiohttp import web
from carrot_server_dashcam_http.source import selected_source

FUNCTIONS = frozenset(('client_replay_source_description', 'api_dashcam_summary_source',
  'api_dashcam_replay_source', 'api_dashcam_replay_source_file', 'api_dashcam_download'))

def source(root: Path, state: Path, mime_files: list[Path]):
  routes, previous = selected_source(root, state, [1.], 1700001000)
  from openpilot.selfdrive.carrot.server.features.dashcam import catalog, paths, summary_sources
  paths.DASHCAM_CACHE_DIR = str(state.parent.parent / 'source-cache')
  routes.__dict__.update(quote=quote, mimetypes=mimetypes,
    safe_segment=paths.safe_segment, segment_dir=paths.segment_dir, segment_index=paths.segment_index,
    source_rlog=catalog.source_rlog, source_qlog=catalog.source_qlog, source_video=catalog.source_video,
    build_route_summary_source=summary_sources.build_route_summary_source)
  path = Path('openpilot/selfdrive/carrot/server/features/dashcam/routes.py')
  text = path.read_text(); tree = ast.parse(text, filename=str(path))
  nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name in FUNCTIONS]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), routes.__dict__)
  mimetypes.knownfiles = [str(path) for path in mime_files]; mimetypes.inited = False; mimetypes._db = None
  proof = {'prior_original_functions': previous, 'additional_functions': [
    {'name': node.name, 'line': node.lineno, 'source_segment_sha256': hashlib.sha256(ast.get_source_segment(text,node).encode()).hexdigest()} for node in nodes],
    'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
    'summary_sources_sha256': hashlib.sha256(Path(summary_sources.__file__).read_bytes()).hexdigest(),
    'provider_remapping': {'DASHCAM_ROOT': str(root), 'DASHCAM_CACHE_DIR': paths.DASHCAM_CACHE_DIR, 'CARROT_DASHCAM_READ_STATE_PATH': str(state), 'mimetypes.knownfiles': list(mimetypes.knownfiles)},
    'method': 'Unchanged original AST bodies and imported unchanged summary_sources.py; only explicit owned root/state/MIME provider remapping'}
  return routes, proof

def application(routes: ModuleType) -> web.Application:
  app = web.Application()
  for path, name in (('summary-source/{route}', 'api_dashcam_summary_source'),
      ('replay-source/{segment}', 'api_dashcam_replay_source'),
      ('replay-source/{segment}/{kind}', 'api_dashcam_replay_source_file'),
      ('download/{segment}/{kind}', 'api_dashcam_download')):
    app.router.add_get('/api/dashcam/' + path, getattr(routes, name))
  return app
