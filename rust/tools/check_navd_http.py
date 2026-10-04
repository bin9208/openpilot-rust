from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import threading
import time
import zlib

import brotli
import requests


class Server:
  def __init__(self, responses):
    self.responses = list(responses)
    self.rows = []
    owner = self

    class Handler(BaseHTTPRequestHandler):
      protocol_version = 'HTTP/1.1'

      def log_message(self, *_args):
        pass

      def do_GET(self):
        owner.rows.append({'path': self.path, 'headers': {key.lower(): value for key, value in self.headers.items()}})
        row = owner.responses.pop(0)
        if row.get('disconnect'):
          self.close_connection = True
          return
        body = row.get('body', b'{"routes": [], "label": "navigation"}')
        time.sleep(row.get('header_delay', 0))
        try:
          self.send_response(row.get('status', 200))
          self.send_header('Content-Length', str(len(body)))
          self.send_header('Connection', 'close')
          for key, value in row.get('headers', [('Content-Type', 'application/json')]):
            self.send_header(key, value)
          self.end_headers()
          time.sleep(row.get('body_delay', 0))
          if row.get('drip'):
            for fragment in (body[:1], body[1:2], body[2:]):
              self.wfile.write(fragment)
              self.wfile.flush()
              if len(fragment) == 1:
                time.sleep(row['drip'])
          else:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
          pass
        self.close_connection = True

    self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    self.thread = threading.Thread(target=self.server.serve_forever, kwargs={'poll_interval': .01}, daemon=True)
    self.thread.start()
    self.url = f'http://127.0.0.1:{self.server.server_port}/route?token=fixture&language=ko'

  def close(self):
    self.server.shutdown()
    self.server.server_close()
    self.thread.join()


def cases():
  output = [('plain', [{}]), ('created', [{'status': 201}]), ('unauthorized', [{'status': 401}]),
            ('server_error', [{'status': 503}]), ('invalid_json', [{'body': b'not JSON'}]),
            ('disconnect', [{'disconnect': True}]), ('empty', [{'body': b''}])]
  body = '{"routes": [], "label": "내비 café"}'.encode()
  encoders = {'gzip': gzip.compress, 'x-gzip': gzip.compress, 'GZip': gzip.compress,
              'br': brotli.compress, 'BR': brotli.compress, 'deflate': zlib.compress,
              'raw_deflate': lambda value: zlib.compress(value)[2:-4],
              'gzip, br': lambda value: brotli.compress(gzip.compress(value)),
              'deflate, gzip': lambda value: gzip.compress(zlib.compress(value))}
  for name, encode in encoders.items():
    output.append((name.replace(', ', '_'), [{'body': encode(body), 'headers': [
      ('Content-Type', 'application/json'), ('Content-Encoding', 'deflate' if name == 'raw_deflate' else name)]}]))
  output.append(('invalid_gzip', [{'body': b'invalid compressed bytes', 'headers': [('Content-Encoding', 'gzip')]}]))
  for encoding, content_type in (('utf-8', None), ('utf-8-sig', None), ('utf-16', None), ('utf-32', None),
                                  ('utf-16-le', None), ('utf-32-be', None), ('cp949', 'application/json; charset=cp949'),
                                  ('utf-8-sig', 'application/json'), ('latin-1', 'text/plain')):
    text = '{"routes": [], "label": "café"}' if encoding == 'latin-1' else '{"routes": [], "label": "내비"}'
    output.append((f'encoding_{encoding}_{content_type is not None}', [{'body': text.encode(encoding),
                   'headers': [] if content_type is None else [('Content-Type', content_type)]}]))
  for status in (301, 302, 303, 307, 308):
    output.append((f'redirect_{status}', [{'status': status, 'headers': [('Location', '/next?q=%ED%95%9C')]}, {}]))
  output.append(('cookie_paths', [
    {'status': 302, 'headers': [('Location', '/next/page'), ('Set-Cookie', 'root=first; Path=/'),
      ('Set-Cookie', 'specific=second; Path=/next'), ('Set-Cookie', 'secret=hidden; Secure; Path=/')]},
    {'status': 307, 'headers': [('Location', '/next/final'), ('Set-Cookie', 'root=updated; Path=/')]}, {}]))
  output.append(('redirect_limit', [{'status': 302, 'headers': [('Location', '/again')]}] * 31))
  return output


def source(url):
  try:
    response = requests.get(url, timeout=10)
    result = {'ok': True, 'status': response.status_code, 'text': response.text}
    try:
      result.update(json_ok=True, json=response.json())
    except requests.exceptions.JSONDecodeError:
      result['json_ok'] = False
    return result
  except requests.RequestException as error:
    return {'ok': False, 'error': repr(error)}


def compare(case, binary, output, runner):
  name, responses = case
  path = output / name
  path.mkdir()
  records = []
  for implementation in ('source', 'native'):
    server = Server(responses)
    started = time.monotonic()
    try:
      if implementation == 'source':
        result = source(server.url)
      else:
        process = subprocess.run([*runner, str(binary)], input=json.dumps(server.url) + '\n', text=True,
                                 capture_output=True, check=False, timeout=25)
        (path / 'native.stdout').write_text(process.stdout)
        (path / 'native.stderr').write_text(process.stderr)
        assert process.returncode == 0, (name, process.returncode, process.stderr)
        result = json.loads(process.stdout)
      elapsed = time.monotonic() - started
      rows = server.rows
      for row in rows:
        row['headers']['host'] = '<loopback>'
      records.append({'result': result, 'requests': rows, 'elapsed': elapsed})
    finally:
      server.close()
  (path / 'records.json').write_text(json.dumps(records, indent=2, ensure_ascii=False) + '\n')
  expected, actual = records
  for record in records:
    record['result'].pop('error', None)
  passed = expected['result'] == actual['result'] and expected['requests'] == actual['requests']
  if name in ('header_timeout', 'body_timeout'):
    passed &= all(not row['result']['ok'] and 9.5 <= row['elapsed'] < 13 for row in records)
  if name == 'progress_beyond_total_timeout':
    passed &= all(row['result']['ok'] and 11 <= row['elapsed'] < 16 for row in records)
  return {'name': name, 'passed': passed}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--timeouts', action='store_true')
  parser.add_argument("--runner", nargs=argparse.REMAINDER, default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  corpus = cases()
  if args.timeouts:
    corpus.extend([('header_timeout', [{'header_delay': 11}]), ('body_timeout', [{'body_delay': 11}]),
                   ('progress_beyond_total_timeout', [{'drip': 5.5}])])
  with ThreadPoolExecutor(max_workers=4) as executor:
    results = list(executor.map(lambda case: compare(case, args.binary.resolve(), args.output, args.runner), corpus))
  root = Path(__file__).resolve().parents[2]
  files = [Path(__file__), args.binary, root / 'openpilot/selfdrive/navd/navd.py',
           root / 'rust/crates/http-transport/src/lib.rs', *sorted((root / 'rust/crates/navd').rglob('*.rs'))]
  report = {'status': 'PASS' if all(row['passed'] for row in results) else 'FAIL',
            'cases': results, 'requests_version': requests.__version__,
            'files': {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in files}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'status': report['status'], 'cases': len(results), 'failures': [row['name'] for row in results if not row['passed']]}))
  raise SystemExit(report['status'] != 'PASS')


if __name__ == '__main__':
  main()
