"""Original Prime/Firehose class bodies against owned loopback HTTP and real native Params."""

import argparse
import ast
from datetime import datetime, timedelta, UTC
from enum import IntEnum
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib
import json
from pathlib import Path
import subprocess
import tempfile
import sys
import threading
import time
from types import ModuleType, SimpleNamespace
import jwt
import requests

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--persist', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root))
hardware = ModuleType('openpilot.system.hardware.hw')
hardware.Paths = SimpleNamespace(persist_root=lambda: str(args.persist))
sys.modules[hardware.__name__] = hardware
version = ModuleType('openpilot.system.version')
version.get_version = lambda: (root / 'openpilot/common/version.h').read_text().split('"')[1]
sys.modules[version.__name__] = version
api = importlib.import_module('openpilot.common.api')
helper = importlib.import_module('openpilot.selfdrive.ui.lib.api_helpers')
clock = importlib.import_module('openpilot.common.time_helpers')


class FixedDatetime(datetime):
  @classmethod
  def now(cls, tz=None):
    value = datetime.fromtimestamp(1790812800, UTC)
    return value if tz else value.replace(tzinfo=None)


api.datetime = FixedDatetime
helper.time = SimpleNamespace(monotonic=lambda: 1.0)
clock.datetime = SimpleNamespace(datetime=FixedDatetime, timedelta=timedelta)
clock.min_date = lambda: datetime(2025, 2, 21)
algorithm, _, public = api.get_key_pair()
records, responses, errors = [], [], []


class Handler(BaseHTTPRequestHandler):
  protocol_version = 'HTTP/1.1'

  def log_message(self, *unused):
    pass

  def do_GET(self):
    claims = jwt.decode(
      self.headers['Authorization'].removeprefix('JWT '),
      public,
      algorithms=[algorithm],
      options={'verify_exp': False, 'verify_iat': False, 'verify_nbf': False},
    )
    records.append({'path': self.path, 'claims': claims, 'agent': self.headers.get('User-Agent')})
    status, body = responses.pop(0)
    body = body.encode()
    self.send_response(status)
    self.send_header('Content-Type', 'application/json')
    self.send_header('Content-Length', str(len(body)))
    self.end_headers()
    self.wfile.write(body)


server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
api.API_HOST = f'http://127.0.0.1:{server.server_port}'
values = {}


class Params:
  def get(self, key):
    value = values.get(key)
    if value is None or not value:
      return None
    try:
      if key == 'PrimeType':
        return int(value.encode())
      if key == 'ApiCache_FirehoseStats':
        return json.loads(value)
      return value
    except (ValueError, TypeError):
      return None

  def put(self, key, value):
    values[key] = json.dumps(value) if isinstance(value, (dict, list)) else str(value)


class DummyThread:
  def __init__(self, **kwargs):
    pass

  def start(self):
    pass

  def is_alive(self):
    return False


class Widget:
  pass


namespace = {
  'IntEnum': IntEnum,
  'os': SimpleNamespace(getenv=lambda key: environment),
  'requests': requests,
  'threading': SimpleNamespace(Lock=threading.Lock, Thread=DummyThread),
  'time': time,
  'Params': Params,
  'cloudlog': SimpleNamespace(error=lambda value: errors.append(value), exception=lambda value: errors.append(value), info=lambda value: None),
  'UNREGISTERED_DONGLE_ID': 'UnregisteredDevice',
  'get_token': helper.get_token,
  'api_get': api.api_get,
  'Widget': Widget,
  'rl': SimpleNamespace(Color=lambda *value: value, Rectangle=object),
  'GuiScrollPanel2': lambda **unused: None,
}
for path, classes in [
  ('openpilot/selfdrive/ui/lib/prime_state.py', {'PrimeType', 'PrimeState'}),
  ('openpilot/selfdrive/ui/mici/layouts/settings/firehose.py', {'FirehoseLayoutBase'}),
]:
  source = ast.parse((root / path).read_text())
  nodes = [node for node in source.body if isinstance(node, ast.ClassDef) and node.name in classes]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), path, 'exec'), namespace)
results = []
prime_payloads = [
  (200, '{}'),
  (200, '{"is_paired":true}'),
  (200, '{"is_paired":true,"prime_type":2}'),
  (200, '{"is_paired":true,"prime_type":99}'),
  (200, '{"is_paired":false}'),
  (200, '{"is_paired":[],"prime_type":99}'),
  (200, '[]'),
  (200, 'not-json'),
  (500, 'failure'),
  (200, '{"is_paired":"yes","prime_type":true}'),
  (200, '{"is_paired":true,"prime_type":2.0}'),
  (200, '{"is_paired":true,"prime_type":NaN}'),
]
firehose_payloads = [
  (200, '{}'),
  (200, '{"firehose":4,"label":"한글"}'),
  (200, '{"firehose":2.75}'),
  (200, '{"firehose":"9"}'),
  (200, '{"firehose":true}'),
  (200, '{"firehose":null}'),
  (200, '{"firehose":NaN}'),
  (200, '[]'),
  (200, 'invalid'),
  (404, 'not found'),
  (200, '{"firehose":1234567890123456789012345678901234567890}'),
]
cases = [('prime-main', 'prime', None, None, prime_payloads), ('firehose-main', 'firehose', '{"firehose":"０_１"}', None, firehose_payloads)]
for index, (raw, env) in enumerate(
  [(None, None), ('1x', None), ('2', None), (None, '０_１'), ('-1', ''), (None, '99'), ('2', '+3'), (None, '_1'), (None, '1__0'), ('１', None)]
):
  cases.append((f'prime-initial-{index}', 'prime', raw, env, []))
for index, raw in enumerate(
  [None, '[]', 'null', '{"firehose":2.75}', '{"firehose":true}', '{"firehose":NaN}', '{"firehose":"no"}', '{"firehose":123456789012345678901234567890}']
):
  cases.append((f'firehose-initial-{index}', 'firehose', raw, None, []))
try:
  for name, mode, initial, environment, payloads in cases:
    key = 'PrimeType' if mode == 'prime' else 'ApiCache_FirehoseStats'
    values = {key: initial} if initial is not None else {}
    identities = ['', 'UnregisteredDevice'] + [f'device{index}' for index in range(len(payloads))] if payloads else []
    scene = {'mode': mode, 'initial': initial, 'environment': environment, 'identities': identities}
    path = args.output / f'{name}-input.json'
    path.write_text(json.dumps(scene))
    records.clear()
    responses[:] = payloads
    errors.clear()
    helper._get_token.cache_clear()
    worker = namespace['PrimeState']() if mode == 'prime' else namespace['FirehoseLayoutBase']()

    def snapshot(error, worker=worker, mode=mode, values=values, key=key):
      value = {'value': int(worker.prime_type)} if mode == 'prime' else {'value_json': json.dumps(worker._segment_count)}
      return dict(value, params=values.get(key) or '', error=error)

    expected = [snapshot(False)]
    for identity in identities:
      values['DongleId'] = identity
      errors.clear()
      worker._fetch_prime_status() if mode == 'prime' else worker._fetch_firehose_stats()
      expected.append(snapshot(bool(errors)))
    source_records = list(records)
    records.clear()
    responses[:] = payloads
    result = subprocess.run(
      [str(args.binary), str(root), str(args.persist), api.API_HOST, str(path), tempfile.mkdtemp(prefix=f'{name}-native-', dir=args.output)],
      capture_output=True,
      text=True,
      check=True,
    )
    actual = json.loads(result.stdout)
    native_records = list(records)
    (args.output / f'{name}-source.json').write_text(json.dumps({'states': expected, 'requests': source_records}, indent=2))
    (args.output / f'{name}-native.json').write_text(json.dumps({'states': actual, 'requests': native_records}, indent=2))
    assert expected == actual, (name, expected, actual)
    assert source_records == native_records, (name, source_records, native_records)
    results.append({'case': name, 'states': len(actual), 'requests': len(native_records), 'exact': True})
finally:
  server.shutdown()
  server.server_close()
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
print('PASS: original/native Prime and Firehose fetch state, Params bytes, authenticated HTTP metadata and initial typed values')
