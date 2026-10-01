#!/usr/bin/env python3
"""Load unchanged Athena AST bodies with private filesystem/Params/hardware seams."""

import ast
import json
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]


class ParamsStore:
  def __init__(self):
    self.values = {}
    self.writes = []

  def __call__(self):
    return self

  def get(self, key):
    return self.values.get(key)

  def put(self, key, value):
    self.values[key] = value
    self.writes.append([key, value])

  def get_bool(self, key):
    return self.values.get(key, False)

  def put_bool(self, key, value):
    self.put(key, value)

  def remove(self, key):
    self.values.pop(key, None)
    self.writes.append([key, None])


def source(log_root: Path, params: ParamsStore):
  from openpilot.common.utils import CallbackReader, get_upload_stream
  original = ast.parse((ROOT / 'openpilot/system/athena/athenad.py').read_text())
  nodes = []
  for node in original.body:
    if isinstance(node, ast.ImportFrom) and node.module.startswith('openpilot'):
      continue
    if isinstance(node, ast.Import) and any(alias.name.startswith('openpilot') for alias in node.names):
      continue
    if isinstance(node, ast.If) and ast.unparse(node.test) == "__name__ == '__main__'":
      continue
    nodes.append(node)
  paths = SimpleNamespace(log_root=lambda: str(log_root), swaglog_root=lambda: str(log_root / 'logs'), stats_root=lambda: str(log_root / 'stats'))
  scope = {
    '__name__': 'athena_source_reference',
    'Params': params,
    'Paths': paths,
    'PC': True,
    'log': SimpleNamespace(DeviceState=SimpleNamespace(NetworkType=None)),
    'cloudlog': SimpleNamespace(event=lambda *a, **k: None, exception=lambda *a, **k: None, debug=lambda *a, **k: None),
    'HARDWARE': None,
    'SERVICE_LIST': {},
    'CallbackReader': CallbackReader,
    'get_upload_stream': get_upload_stream,
  }
  # Dataclasses resolve postponed annotations through the owning module.
  import sys
  import types
  module = types.ModuleType(scope['__name__'])
  module.__dict__.update(scope)
  sys.modules[module.__name__] = module
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(ROOT / 'openpilot/system/athena/athenad.py'), 'exec'), module.__dict__)
  return module.__dict__


def policy_trace(root: Path, operations: list[dict]):
  params = ParamsStore()
  scope = source(root, params)
  clock = scope['time']
  scope['time'] = SimpleNamespace(time=lambda: 1720000000.125, sleep=lambda _: None, monotonic=clock.monotonic)
  records = []
  for row in operations:
    op = row['op']
    try:
      if op == 'params':
        params.values.update(row['values'])
        result = None
      elif op == 'initialize':
        result = scope['UploadQueueCache'].initialize(scope['upload_queue'])
      elif op == 'pop':
        result = scope['asdict'](scope['upload_queue'].get_nowait())
      elif op == 'current':
        scope['cur_upload_items'][row['tid']] = scope['UploadItem'].from_dict(row['item'])
        result = None
      elif op == 'cache':
        result = scope['UploadQueueCache'].cache(scope['upload_queue'])
      elif op == 'retry':
        import threading
        result = scope['retry_upload'](row['tid'], threading.Event(), row['increase'])
      elif op == 'cancelFirst':
        result = scope['cancelUpload'](scope['listUploadQueue']()[0]['id'])
      else:
        result = scope[op](*row.get('args', []), **row.get('kwargs', {}))
      outcome = {'result': result}
    except (TypeError, ValueError, KeyError, OSError, IndexError, AttributeError) as error:
      outcome = {'error': type(error).__name__, 'message': str(error)}
    records.append({'op': op, **outcome, 'queue': scope['listUploadQueue'](), 'params': json.loads(json.dumps(params.values))})
  return records
