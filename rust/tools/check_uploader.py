#!/usr/bin/env python3
"""Compare native uploader decisions and local HTTP uploads with the original source.

All keys, Params stand-ins and log files are synthetic. The only HTTP endpoint is
an ephemeral loopback server. This does not import runtime hardware initialization.
"""

import argparse
import ast
import contextlib
import datetime
import io
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile
import threading
import time
import traceback
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from urllib.parse import parse_qs, urlparse

import jwt
import requests
import zstandard
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa


def definitions(path, names):
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
  return compile(tree, str(path), 'exec')


def source(root, config, http=None):
  events = []
  cloudlog = SimpleNamespace(event=lambda name, **kw: events.append(name), exception=lambda name: events.append(name), debug=lambda *args: None)
  attrs = {'__name__': 'uploader_oracle_xattr'}
  exec(compile((root / 'openpilot/system/loggerd/xattr_cache.py').read_text(), 'xattr_cache.py', 'exec'), attrs)
  namespace = {
    'os': os,
    'json': json,
    'time': time,
    'traceback': traceback,
    'datetime': datetime,
    'Iterator': Iterator,
    'cloudlog': cloudlog,
    'UPLOAD_ATTR_NAME': 'user.upload',
    'UPLOAD_ATTR_VALUE': b'1',
    'MAX_UPLOAD_SIZES': {'qlog': 25e6, 'qcam': 5e6},
    'getxattr': attrs['getxattr'],
    'setxattr': attrs['setxattr'],
    'Params': lambda: SimpleNamespace(get=lambda key: config.get('requested')),
    'Api': lambda dongle: None,
    'fake_upload': False,
  }
  if config.get('fail_mark'):

    def failed_mark(*args):
      raise PermissionError(13, 'fixture')

    namespace['setxattr'] = failed_mark
  exec(
    definitions(
      root / 'openpilot/system/loggerd/uploader.py', {'get_directory_sort', 'listdir_by_creation', 'clear_locks', 'FakeRequest', 'FakeResponse', 'Uploader'}
    ),
    namespace,
  )
  uploader = namespace['Uploader']('0000000000000000', config['root'])
  if http:
    namespace.update(io=io, zstd=zstandard, LOG_COMPRESSION_LEVEL=10)
    exec(definitions(root / 'openpilot/common/utils.py', {'get_upload_stream'}), namespace)
    api_scope = {
      'os': os,
      'jwt': jwt,
      'datetime': datetime.datetime,
      'timedelta': datetime.timedelta,
      'UTC': datetime.UTC,
      'Paths': SimpleNamespace(persist_root=lambda: http['persist']),
      'API_HOST': http['api_host'],
      'BASEDIR': str(root),
      'requests': SimpleNamespace(request=lambda method, url, timeout, **kw: requests.request(method, url, timeout=http.get('timeout_ms', 10000) / 1000, **kw)),
    }
    exec(definitions(root / 'openpilot/system/version.py', {'get_version'}), api_scope)
    api_tree = ast.parse((root / 'openpilot/common/api.py').read_text())
    api_tree.body = [
      node
      for node in api_tree.body
      if isinstance(node, (ast.FunctionDef, ast.ClassDef))
      or isinstance(node, ast.Assign)
      and any(isinstance(target, ast.Name) and target.id == 'KEYS' for target in node.targets)
    ]
    exec(compile(api_tree, str(root / 'openpilot/common/api.py'), 'exec'), api_scope)
    uploader.api = api_scope['Api']('0000000000000000')
    namespace['requests'] = SimpleNamespace(
      put=lambda url, data, headers, timeout: requests.put(url, data=data, headers=headers, timeout=http.get('timeout_ms', 10000) / 1000)
    )
    namespace['fake_upload'] = http.get('fake', False)
  else:

    def transfer(key, fn):
      if config.get('fail_transfer'):
        raise RuntimeError('fixture failure')
      return SimpleNamespace(status_code=config.get('status', 200), request=SimpleNamespace(headers={'Content-Length': config.get('length', '42')}))

    uploader.do_upload = transfer
  return uploader, events


def observe(uploader, events, config):
  metered = config.get('metered', False)
  files = [list(row[:2]) for row in uploader.list_upload_files(metered)]
  selected = uploader.next_file_to_upload(metered)
  result = {'files': files, 'next': list(selected[:2]) if selected else None, 'results': [], 'last': '', 'events': events}

  def record(call):
    try:
      result['results'].append(call())
    except (UnboundLocalError, ValueError) as error:
      result['results'].append({'error': type(error).__name__})
      return False
    return True

  if config.get('upload'):
    key = config['upload']
    record(lambda: uploader.upload(Path(key).name, key, str(Path(config['root']) / key), 1, metered))
  for _ in range(config.get('steps', 0)):
    if not record(lambda: uploader.step(1, metered)):
      break
  result['last'] = os.path.relpath(uploader.last_filename, config['root']) if uploader.last_filename else ''
  return result


def native(binary, config):
  result = subprocess.run([str(binary)], input=json.dumps(config) + '\n', text=True, capture_output=True, check=True)
  return json.loads(result.stdout)


def make_files(root, files):
  for key, size, uploaded in files:
    path = root / key
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('wb') as stream:
      if size > 100000:
        stream.truncate(size)
      else:
        stream.write(bytes((i * 23 + 17) % 256 for i in range(size)))
    if uploaded:
      os.setxattr(path, 'user.upload', b'1')


def compare(root, binary, work, number, files, config, http=None):
  original = work / f'{number}-original'
  rust = work / f'{number}-native'
  original.mkdir()
  rust.mkdir()
  make_files(original, files)
  make_files(rust, files)
  if config.get('readonly'):
    for key, _, _ in files:
      (original / key).chmod(0o444)
      (rust / key).chmod(0o444)
  reference_config = {**config, 'root': str(original)}
  uploader, events = source(root, reference_config, http)
  expected = observe(uploader, events, reference_config)
  actual = native(binary, {**config, 'root': str(rust), **({'http': http} if http else {})})
  assert actual == expected, (number, config, expected, actual)
  for key, _, _ in files:

    def uploaded(path):
      try:
        return os.getxattr(path, 'user.upload')
      except OSError:
        return None

    assert uploaded(original / key) == uploaded(rust / key), (number, key, 'xattr')
  return actual


def policy(root, binary, work):
  rng = random.Random(502026)
  cases = []
  for _ in range(180):
    files = []
    for directory in rng.sample(
      ['2024-10-01--12-30-00--0', '00000abc--route--0', '00000abc--route--10', '00000abc--route--2', 'crash', 'boot', '한글--3'], rng.randint(1, 7)
    ):
      for name in rng.sample(['qlog', 'qlog.zst', 'rlog', 'qcamera.ts', 'fcamera.hevc', 'fixture.lock', 'plain'], rng.randint(1, 7)):
        files.append((directory + '/' + name, rng.choice([0, 1, 1024]), rng.random() < 0.2))
    config = {
      'metered': rng.choice([True, False]),
      'requested': rng.choice([None, '', 'dongle|00000abc--route', ',dongle|,', '한글', 'other']),
      'status': rng.choice([200, 201, 401, 403, 412, 202, 400, 500]),
      'steps': 12,
      'fail_transfer': rng.random() < 0.1,
    }
    cases.append((files, config))
  for size in [0, 1, 5_000_000, 5_000_001, 25_000_000, 25_000_001]:
    for name in ['qlog', 'qcam', 'qcamera.ts', 'qlog.zst']:
      cases.append(([('route--0/' + name, size, False)], {'upload': 'route--0/' + name, 'fail_mark': size in [0, 25_000_001]}))
  for status in [200, 201, 401, 403, 412, 500]:
    cases.append(([('route--0/qlog', 8, False)], {'steps': 2, 'status': status, 'length': 'invalid', 'fail_mark': True}))
  for size in [0, 25_000_001]:
    cases.append(([('route--0/qlog', size, False)], {'steps': 1, 'readonly': True}))
  for index, (files, config) in enumerate(cases):
    compare(root, binary, work, f'policy-{index}', files, config)
  inputs, expected = [], []
  tree = ast.parse((root / 'openpilot/system/loggerd/uploader.py').read_text())
  main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  initial = next(
    node for node in main.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'backoff' for target in node.targets)
  )
  loop = next(node for node in main.body if isinstance(node, ast.While))
  backoff_scope = {'cloudlog': SimpleNamespace(info=lambda *args: None), 'allow_sleep': True, 'time': SimpleNamespace(sleep=expected.append)}
  exec(compile(ast.Module(body=[initial], type_ignores=[]), 'uploader.py', 'exec'), backoff_scope)
  backoff_code = compile(ast.Module(body=loop.body[-2:], type_ignores=[]), 'uploader.py', 'exec')
  for _ in range(10000):
    success, offroad, jitter = rng.choice([None, True, False]), rng.choice([True, False]), rng.random()
    inputs.append([success, offroad, jitter])
    backoff_scope.update(
      success=success, offroad=offroad, random=SimpleNamespace(uniform=lambda lower, upper, fraction=jitter: lower + (upper - lower) * fraction)
    )
    exec(backoff_code, backoff_scope)
  assert native(binary, {'root': str(work), 'backoff': inputs}) == expected
  return {'filesystem_scenarios': len(cases), 'backoff_decisions': len(inputs)}


def signing_key(persist, name):
  key = rsa.generate_private_key(public_exponent=65537, key_size=2048) if name == 'id_rsa' else ec.generate_private_key(ec.SECP256R1())
  path = persist / 'comma' / name
  path.parent.mkdir(parents=True, exist_ok=True)
  path.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL, serialization.NoEncryption()))
  path.with_suffix('.pub').write_bytes(key.public_key().public_bytes(serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo))
  return key.public_key()


@contextlib.contextmanager
def endpoint(settings, captures):
  class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
      pass

    def reply(self, status, body=b'', **headers):
      self.send_response(status)
      self.send_header('Content-Length', str(len(body)))
      for name, value in headers.items():
        self.send_header(name, value)
      self.end_headers()
      try:
        if settings.get('slow_body'):
          for offset in range(0, len(body), max(1, len(body) // 4)):
            self.wfile.write(body[offset : offset + max(1, len(body) // 4)])
            self.wfile.flush()
            time.sleep(0.08)
        else:
          self.wfile.write(body)
      except (BrokenPipeError, ConnectionResetError):
        pass

    def do_GET(self):
      captures.append(('GET', self.path, dict(self.headers), b''))
      if self.path.startswith('/v1.4/'):
        if settings.get('stall'):
          time.sleep(0.4)
        url = f'http://127.0.0.1:{self.server.server_port}/put'
        body = json.dumps({'url': url, 'headers': {'X-Fixture': 'synthetic'}}).encode()
        self.reply(settings.get('api_status', 200), b'invalid-json' if settings.get('bad_json') else body)
      else:
        self.reply(settings.get('put_status', 200), b'done')

    def do_PUT(self):
      body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
      captures.append(('PUT', self.path, dict(self.headers), body))
      if self.path == '/put' and settings.get('redirect'):
        self.reply(settings['redirect'], Location='/final')
      else:
        self.reply(settings.get('put_status', 200), b'uploaded')

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=server.serve_forever, daemon=True)
  thread.start()
  try:
    yield f'http://127.0.0.1:{server.server_port}'
  finally:
    server.shutdown()
    server.server_close()
    thread.join()


def http_cases(root, binary, work):
  total = 0
  variants = [
    {},
    {'api_status': 412, 'bad_json': True},
    {'bad_json': True},
    {'api_status': 500},
    {'fake': True},
    {'put_status': 201},
    {'put_status': 401},
    {'put_status': 403},
    {'put_status': 412},
    {'put_status': 500},
    {'redirect': 307},
    {'redirect': 308},
    {'redirect': 302},
    {'redirect': 301},
    {'stall': True},
    {'slow_body': True},
  ]
  for name in ['id_rsa', 'id_ecdsa']:
    persist = work / name
    public = signing_key(persist, name)
    for settings in variants:
      captures = []
      with endpoint(settings, captures) as host:
        http = {
          'api_host': host,
          'persist': str(persist),
          'version': str(root / 'openpilot/common/version.h'),
          'key_name': name,
          'fake': settings.get('fake', False),
          'timeout_ms': 200 if settings.get('stall') or settings.get('slow_body') else 10000,
        }
        compare(root, binary, work, f'http-{total}', [('route--0/qlog', 524288, False)], {'steps': 1}, http)
      assert len(captures) % 2 == 0, captures
      count = len(captures) // 2
      for left, right in zip(captures[:count], captures[count:], strict=True):
        assert left[:2] == right[:2], (settings, left[:2], right[:2])
        assert left[3] == right[3], (settings, 'different transmitted bytes', len(left[3]), len(right[3]))
        if left[0] == 'GET' and left[1].startswith('/v1.4/'):
          assert parse_qs(urlparse(left[1]).query)['path'] == ['route--0/qlog.zst']
          for capture in [left, right]:
            token = next(value for key, value in capture[2].items() if key.lower() == 'authorization').removeprefix('JWT ')
            payload = jwt.decode(token, public, algorithms=['RS256' if name == 'id_rsa' else 'ES256'])
            assert payload['identity'] == '0000000000000000'
            assert payload['nbf'] == payload['iat'] and payload['exp'] - payload['iat'] == 3600
        elif left[0] == 'PUT' and left[3]:
          expected = b'\0' * 524288
          assert zstandard.ZstdDecompressor().stream_reader(io.BytesIO(left[3])).read() == expected
      total += 1
  return {'loopback_http_scenarios': total, 'jwt_algorithms': ['RS256', 'ES256'], 'compressed_payloads': 'byte-exact source/native'}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
  args = parser.parse_args()
  with tempfile.TemporaryDirectory(prefix='uploader-oracle-') as directory:
    work = Path(directory)
    report = {**policy(args.root, args.binary, work), **http_cases(args.root, args.binary, work)}
  report['passed'] = True
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report, indent=2))


if __name__ == '__main__':
  main()
