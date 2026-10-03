#!/usr/bin/env python3
"""Exercise the original ten-second socket deadlines using loopback only."""

import argparse
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import threading
import time

from check_uploader import make_files, native, observe, signing_key, source


def scenario(root, binary, output, stage, mode, implementation):
  directory = output / f'{stage}-{mode}-{implementation}'
  directory.mkdir(parents=True)
  persist = directory / 'persist'
  signing_key(persist, 'id_rsa')
  logs = directory / 'logs'
  make_files(logs, [('route--0/qlog', 128, False)])
  captures = []

  class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
      pass

    def respond(self, body, current):
      active = current == stage.split('-')[0]
      delay = 11.0 if mode == 'late' else 12.0
      if active and stage.endswith('headers'):
        time.sleep(delay)
      self.send_response(200)
      self.send_header('Content-Length', str(len(body)))
      self.end_headers()
      try:
        if active and mode == 'progress':
          # Each gap remains below ten seconds, while total response time exceeds it.
          for part in [body[:1], body[1:2], body[2:]]:
            self.wfile.write(part)
            self.wfile.flush()
            if len(part) == 1:
              time.sleep(5.5)
        else:
          if active and stage.endswith('body'):
            time.sleep(delay)
          self.wfile.write(body)
          self.wfile.flush()
      except (BrokenPipeError, ConnectionResetError):
        pass

    def do_GET(self):
      captures.append({'method': 'GET', 'path': self.path, 'time': time.monotonic()})
      body = json.dumps({'url': f'http://127.0.0.1:{self.server.server_port}/put', 'headers': {}}).encode()
      self.respond(body, 'get')

    def do_PUT(self):
      body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
      captures.append({'method': 'PUT', 'path': self.path, 'body_hex': body.hex(), 'time': time.monotonic()})
      self.respond(b'uploaded', 'put')

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=server.serve_forever, daemon=True)
  thread.start()
  config = {'root': str(logs), 'steps': 1}
  http = {'api_host': f'http://127.0.0.1:{server.server_port}', 'persist': str(persist), 'version': str(root / 'openpilot/common/version.h')}
  start = time.monotonic()
  try:
    if implementation == 'python':
      uploader, events = source(root, config, http)
      result = observe(uploader, events, config)
    else:
      result = native(binary, {**config, 'http': http})
    elapsed = time.monotonic() - start
  finally:
    server.shutdown()
    server.server_close()
    thread.join()
  try:
    attribute = os.getxattr(logs / 'route--0/qlog', 'user.upload').hex()
  except OSError:
    attribute = None
  record = {'stage': stage, 'mode': mode, 'implementation': implementation, 'elapsed': elapsed, 'result': result, 'attribute': attribute, 'requests': captures}
  (directory / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
  return record


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
  parser.add_argument('--late-only', action='store_true')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  cases = [(stage, mode) for stage in ['get-headers', 'get-body', 'put-headers', 'put-body'] for mode in (['late'] if args.late_only else ['late', 'stall'])]
  if not args.late_only:
    cases += [('get-body', 'progress'), ('put-body', 'progress')]
  with ThreadPoolExecutor(max_workers=8) as pool:
    futures = [
      pool.submit(scenario, args.root, args.binary, args.output, stage, mode, implementation) for stage, mode in cases for implementation in ['python', 'rust']
    ]
    records = [future.result() for future in futures]
  failures = []
  for left, right in zip(records[::2], records[1::2], strict=True):
    expected = left['mode'] == 'progress'
    for record in [left, right]:
      if record['result']['results'] != [expected] or record['attribute'] != ('31' if expected else None):
        failures.append(record)
    if left['result'] != right['result']:
      failures.append({'different': [left, right]})
    left_requests = [{key: value for key, value in row.items() if key != 'time'} for row in left['requests']]
    right_requests = [{key: value for key, value in row.items() if key != 'time'} for row in right['requests']]
    if left_requests != right_requests:
      failures.append({'different_requests': [left_requests, right_requests]})
  report = {'passed': not failures, 'scenarios': len(cases), 'executions': len(records), 'records': records, 'failures': failures}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': report['passed'], 'scenarios': len(cases), 'executions': len(records), 'failures': len(failures)}))
  if failures:
    raise SystemExit(1)


if __name__ == '__main__':
  main()
