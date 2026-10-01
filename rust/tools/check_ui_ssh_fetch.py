"""Actual source/native SSH workers against owned local responses and real native Params."""

import argparse
import ast
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
from types import ModuleType, SimpleNamespace
import requests

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root))
swaglog = ModuleType('openpilot.common.swaglog')
swaglog.cloudlog = SimpleNamespace(debug=lambda *args: None, error=lambda *args: None)
sys.modules[swaglog.__name__] = swaglog
from openpilot.system.ui.lib.multilang import load_translations

records = []


class Handler(BaseHTTPRequestHandler):
  def log_message(self, *unused):
    pass

  def do_GET(self):
    records.append({'path': self.path, 'agent': self.headers.get('User-Agent'), 'authorization': self.headers.get('Authorization')})
    status, body = 200, b' \nssh-ed25519 AAAA fixture\n\t'
    if self.path == '/empty.keys':
      body = b' \n\x1c\x1f'
    elif self.path == '/missing.keys':
      status, body = 404, b'missing'
    elif self.path == '/delay.keys':
      time.sleep(16)
    elif self.path == '/unicode.keys':
      body = '\u2003ssh-ed25519 KEY 한글\u001c'.encode()
    try:
      self.send_response(status)
      self.send_header('Content-Type', 'text/plain; charset=utf-8')
      self.send_header('Content-Length', str(len(body)))
      self.end_headers()
      self.wfile.write(body)
    except (BrokenPipeError, ConnectionResetError):
      pass


server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
host = f'http://127.0.0.1:{server.server_port}'
values = {}


class Params:
  def put(self, key, value):
    values[key] = value

  def remove(self, key):
    values.pop(key, None)


source = ast.parse((root / 'openpilot/selfdrive/ui/widgets/ssh_key.py').read_text())
cls = next(node for node in source.body if isinstance(node, ast.ClassDef) and node.name == 'SshKeyFetcher')
results = []
try:
  for language in ['en', 'ko']:
    catalog, _ = load_translations(root / f'openpilot/selfdrive/ui/translations/app_{language}.po') if language != 'en' else ({}, {})
    namespace = {
      'Callable': Callable,
      'Params': Params,
      'threading': threading,
      'tr': lambda text, catalog=catalog: catalog.get(text) or text,
      'requests': SimpleNamespace(get=lambda url, **kwargs: requests.get(url.replace('https://github.com', host, 1), **kwargs), exceptions=requests.exceptions),
    }
    exec(compile(ast.Module(body=[cls], type_ignores=[]), 'ssh_key.py', 'exec'), namespace)
    users = ['valid', 'empty', 'missing', 'unicode'] + (['delay'] if language == 'en' else [])
    scene = {'language': language, 'users': users}
    path = args.output / f'{language}-input.json'
    path.write_text(json.dumps(scene))
    expected = []
    records.clear()

    def response(error, expected=expected):
      expected.append({'error': error, 'username': values.get('GithubUsername', ''), 'keys': values.get('GithubSshKeys', '')})

    fetcher = namespace['SshKeyFetcher'](Params())
    for user in users:
      values.clear()
      values.update({'GithubUsername': 'previous', 'GithubSshKeys': 'previous-keys'})
      count = len(expected)
      fetcher.fetch(user, response)
      deadline = time.monotonic() + 18
      while len(expected) == count:
        fetcher.update()
        assert time.monotonic() < deadline
        time.sleep(0.002)
      fetcher.update()
    source_records = list(records)
    records.clear()
    native_params = tempfile.mkdtemp(prefix=f'{language}-native-', dir=args.output)
    result = subprocess.run([str(args.binary), str(root), host, str(path), native_params], text=True, capture_output=True, check=True)
    actual = json.loads(result.stdout)
    native_records = list(records)
    (args.output / f'{language}-source.json').write_text(json.dumps({'results': expected, 'requests': source_records}, indent=2))
    (args.output / f'{language}-native.json').write_text(json.dumps({'results': actual, 'requests': native_records}, indent=2))
    assert expected == actual, (language, expected, actual)
    assert source_records == native_records, (language, source_records, native_records)
    results.append({'language': language, 'cases': len(users), 'callbacks_params_and_requests_exact': True})
finally:
  server.shutdown()
  server.server_close()
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
print('PASS: 9 real source/native SSH fetch workers, success/empty/HTTP-error/15-second-timeout, Unicode stripping, callback ordering and Korean errors')
