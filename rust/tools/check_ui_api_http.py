"""Owned-loopback original requests/native UI GET session and timeout checks."""

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import threading
import time
import zlib
import requests

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
records = []
other_port = 0


class Handler(BaseHTTPRequestHandler):
  protocol_version = 'HTTP/1.1'

  def log_message(self, *args):
    pass

  def do_GET(self):
    row = {'path': self.path, 'authorization': self.headers.get('Authorization'), 'cookie': self.headers.get('Cookie'), 'agent': self.headers.get('User-Agent')}
    records.append(row)
    status, body, headers = 200, json.dumps(row).encode(), {'Content-Type': 'application/json'}
    if self.path == '/set':
      status, body, headers = 302, b'', {'Location': '/cookie', 'Set-Cookie': 'session=owned; Path=/; HttpOnly'}
    elif self.path == '/cross':
      status, body, headers = 307, b'', {'Location': f'http://127.0.0.1:{other_port}/cross-end'}
    elif self.path == '/latin':
      body, headers = b'caf\xe9', {'Content-Type': 'text/plain'}
    elif self.path == '/utf8':
      body, headers = '한글'.encode(), {'Content-Type': 'text/plain; charset=utf-8'}
    elif self.path == '/deflate':
      body, headers = zlib.compress(b'{"firehose":42}'), {'Content-Type': 'application/json', 'Content-Encoding': 'deflate'}
    elif self.path == '/raw-deflate':
      body, headers = zlib.compress(b'{"firehose":43}')[2:-4], {'Content-Type': 'application/json', 'Content-Encoding': 'deflate'}
    elif self.path == '/missing':
      status, body = 404, b'not found'
    elif self.path == '/loop':
      status, body, headers = 302, b'', {'Location': '/loop'}
    elif self.path == '/headers-delay':
      time.sleep(0.2)
    elif self.path == '/progress':
      body, headers = b'progress', {'Content-Type': 'text/plain'}
    try:
      self.send_response(status)
      for key, value in headers.items():
        self.send_header(key, value)
      self.send_header('Content-Length', str(len(body)))
      self.end_headers()
      if self.path == '/progress':
        for byte in body:
          self.wfile.write(bytes([byte]))
          self.wfile.flush()
          time.sleep(0.025)
      else:
        self.wfile.write(body)
    except (BrokenPipeError, ConnectionResetError):
      pass


servers = [ThreadingHTTPServer(('127.0.0.1', 0), Handler) for _ in range(2)]
for server in servers:
  threading.Thread(target=server.serve_forever, daemon=True).start()
other_port = servers[1].server_port
base = f'http://127.0.0.1:{servers[0].server_port}'
results = []
try:
  for name, paths, timeout in [
    ('session', ['/set', '/cookie', '/cross', '/latin', '/utf8', '/deflate', '/raw-deflate', '/missing'], 1.0),
    ('progress', ['/progress'], 0.08),
    ('timeout', ['/headers-delay'], 0.05),
    ('redirect-limit', ['/loop'], 1.0),
    ('unbounded', ['/utf8'], None),
  ]:
    scene = {'urls': [base + path for path in paths], 'timeout': timeout, 'token': 'fixture'}
    path = args.output / f'{name}-input.json'
    path.write_text(json.dumps(scene))
    records.clear()
    expected = []
    with requests.Session() as session:
      for url in scene['urls']:
        try:
          response = session.get(url, timeout=timeout, headers={'Authorization': 'JWT fixture', 'User-Agent': 'openpilot-fixture'})
          expected.append({'status': response.status_code, 'text': response.text})
        except Exception as error:
          expected.append({'error': type(error).__name__, 'timeout': isinstance(error, requests.exceptions.Timeout)})
    source_records = list(records)
    records.clear()
    result = subprocess.run([str(args.binary), str(path)], capture_output=True, text=True, check=True)
    actual = json.loads(result.stdout)
    native_records = list(records)
    (args.output / f'{name}-source.json').write_text(json.dumps({'responses': expected, 'requests': source_records}, indent=2))
    (args.output / f'{name}-native.json').write_text(json.dumps({'responses': actual, 'requests': native_records}, indent=2))
    for original, native in zip(expected, actual, strict=True):
      if 'error' in original:
        assert 'error' in native and original['timeout'] == native['timeout'], (name, original, native)
      else:
        assert original == native, (name, original, native)
    assert source_records == native_records, (name, source_records, native_records)
    results.append({'scenario': name, 'response_count': len(expected), 'requests': len(source_records), 'exact_observables': True})
finally:
  for server in servers:
    server.shutdown()
    server.server_close()
(args.output / 'results.json').write_text(json.dumps(results, indent=2))
print(
  'PASS: source/native UI GET responses and request metadata; cookie sessions,',
  'cross-port auth stripping, decoding, compression, status, redirects, progress and timeout',
)
