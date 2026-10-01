#!/usr/bin/env python3
import argparse
import gzip
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import threading
import time
import urllib.error
import urllib.request

from check_cweb_policy import module


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  calls = []

  class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
      pass
    def handle_request(self):
      body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
      calls.append({'method': self.command, 'path': self.path, 'body': body.hex(),
                    'user_agent': self.headers.get('User-Agent'), 'content_type': self.headers.get('Content-Type'),
                    'accept_encoding': self.headers.get('Accept-Encoding'), 'connection': self.headers.get('Connection')})
      status, location = 200, None
      result = b'fixture \xed\x95\x9c\xea\xb8\x80\xff'
      if self.path.startswith('/status/'):
        status = int(self.path.rsplit('/', 1)[1])
      if self.path.startswith('/redirect/'):
        status = int(self.path.rsplit('/', 1)[1])
        location = '/target'
      if self.path == '/loop':
        status, location = 301, '/loop'
      if self.path == '/gzip':
        result = gzip.compress(result, mtime=0)
      if self.path == '/timeout':
        time.sleep(.35)
      self.send_response(status)
      self.send_header('Content-Length', str(len(result)))
      if self.path == '/gzip':
        self.send_header('Content-Encoding', 'gzip')
      if location is not None:
        self.send_header('Location', location)
      self.end_headers()
      try:
        if self.path == '/progress':
          for part in [result[:1], result[1:2], result[2:]]:
            self.wfile.write(part)
            self.wfile.flush()
            if len(part) == 1:
              time.sleep(.1)
        else:
          self.wfile.write(result)
      except (BrokenPipeError, ConnectionResetError):
        pass
    do_GET = handle_request
    do_POST = handle_request

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  worker = threading.Thread(target=server.serve_forever)
  worker.start()
  source = module()
  source['urllib'] = urllib
  source_post = source['post_json']
  paths = ['/status/' + str(status) for status in [200, 201, 204, 299, 300, 304, 400, 401, 403, 404, 500]]
  paths += ['/redirect/' + str(status) for status in [301, 302, 303, 307, 308]]
  paths += ['/loop', '/gzip', '/timeout', '/progress']
  results = []
  child = subprocess.Popen([args.binary.resolve()], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  try:
    for path in paths:
      url = f'http://127.0.0.1:{server.server_port}{path}'
      payload = {'deviceId': '한글-fixture', 'ip': '10.1.2.3', 'port': 7000}
      timeout = .15 if path in ['/timeout', '/progress'] else 2.
      calls.clear()
      ok, status, body = source_post(url, payload, timeout)
      expected = {'ok': ok, 'status': status, 'body': body}
      expected_calls = list(calls)
      calls.clear()
      child.stdin.write(json.dumps({'url': url, 'payload': payload, 'timeout': timeout}) + '\n')
      child.stdin.flush()
      actual = json.loads(child.stdout.readline())
      record = {'path': path, 'source': expected, 'native': actual, 'source_calls': expected_calls, 'native_calls': list(calls)}
      results.append(record)
      (args.output / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2))
      assert actual == expected, record
      assert calls == expected_calls, record
    child.stdin.close()
    assert child.wait(timeout=5) == 0, child.stderr.read()
  finally:
    if child.poll() is None:
      child.kill()
      child.wait()
    server.shutdown()
    server.server_close()
    worker.join()
  print(f'PASS {len(results)} actual source/native HTTP status, raw body, redirect, timeout and request comparisons')


if __name__ == '__main__':
  main()
