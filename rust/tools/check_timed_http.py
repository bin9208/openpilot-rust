#!/usr/bin/env python3
"""Original urllib and native timed HTTP with real five-second loopback deadlines."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

from timed_fixtures import environment, native
from timed_reference import Source


def worker(binary, output, mode, implementation):
  captures = []
  with tempfile.TemporaryDirectory(prefix='timed-http-') as temporary, environment(Path(temporary)) as (config, params):
    class Handler(BaseHTTPRequestHandler):
      def log_message(self, *args):
        pass

      def do_GET(self):
        captures.append({'path': self.path, 'agent': self.headers.get('User-Agent'), 'encoding': self.headers.get('Accept-Encoding')})
        redirect = None
        if mode in ['redirect-10', 'redirect-11']:
          index = int(self.path.removeprefix('/r')) if self.path.startswith('/r') else 0
          if index < int(mode.split('-')[1]):
            redirect = f'/r{index + 1}'
        elif mode == 'redirect-loop':
          redirect = '/again'
        elif mode == 'redirect' and self.path != '/final':
          redirect = '/final'
        if redirect is not None:
          self.send_response(302)
          self.send_header('Location', redirect)
          self.send_header('Content-Length', '0')
          self.end_headers()
          return
        body = json.dumps({'status': 'fail' if mode == 'failure-status' else 'success',
                           'timezone': 'missing' if mode == 'invalid-zone' else 'Asia/Seoul'}).encode()
        if mode == 'malformed':
          body = b'{invalid'
        if mode == 'null-zone':
          body = b'{"status":"success","timezone":null}'
        json_cases = {
          'duplicate-status-valid': b'{"status":"fail","status":"success","timezone":"Asia/Seoul"}',
          'duplicate-status-invalid': b'{"status":"success","status":"fail","timezone":"Asia/Seoul"}',
          'duplicate-zone-valid': b'{"status":"success","timezone":"missing","timezone":"Asia/Seoul"}',
          'duplicate-zone-invalid': b'{"status":"success","timezone":"Asia/Seoul","timezone":"missing"}',
          'extra-nan': b'{"status":"success","timezone":"Asia/Seoul","extra":NaN}',
          'extra-infinity': b'{"status":"success","timezone":"Asia/Seoul","extra":Infinity}',
          'extra-negative-infinity': b'{"status":"success","timezone":"Asia/Seoul","extra":-Infinity}',
        }
        body = json_cases.get(mode, body)
        if mode.startswith('headers-'):
          time.sleep(5.05 if mode.endswith('late') else 5.6)
        try:
          self.send_response(503 if mode == 'http-error' else 200)
          self.send_header('Content-Length', str(len(body)))
          self.end_headers()
          if mode == 'progress':
            for chunk in [body[:1], body[1:2], body[2:]]:
              self.wfile.write(chunk)
              self.wfile.flush()
              if len(chunk) == 1:
                time.sleep(2.8)
          else:
            if mode.startswith('body-'):
              time.sleep(5.05 if mode.endswith('late') else 5.6)
            self.wfile.write(body)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
          pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    config['endpoint'] = f'http://127.0.0.1:{server.server_port}/geo'
    config['actions'] = [{'kind': 'internet'}]
    started = time.monotonic()
    try:
      if implementation == 'python':
        value = Source(config, params).action({'kind': 'internet'})
      else:
        rows, records = native(binary, config, output / 'native.jsonl')
        assert not records
        value = rows[0]['result']['ok']
      elapsed = time.monotonic() - started
    finally:
      server.shutdown()
      server.server_close()
      thread.join()
    result = {'mode': mode, 'implementation': implementation, 'value': value, 'elapsed': elapsed, 'requests': captures}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--worker', nargs=2)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  if args.worker:
    worker(args.binary, args.output, *args.worker)
    return
  modes = ['success', 'redirect', 'http-error', 'failure-status', 'invalid-zone', 'null-zone', 'malformed',
           'headers-late', 'headers-stall', 'body-late', 'body-stall', 'progress',
           'duplicate-status-valid', 'duplicate-status-invalid', 'duplicate-zone-valid', 'duplicate-zone-invalid',
           'extra-nan', 'extra-infinity', 'extra-negative-infinity', 'redirect-10', 'redirect-11', 'redirect-loop']

  def run(mode, implementation):
    output = args.output / (mode + '-' + implementation)
    subprocess.run([sys.executable, __file__, '--binary', str(args.binary), '--output', str(output), '--worker', mode, implementation], check=True)
    return json.loads((output / 'result.json').read_text())

  with ThreadPoolExecutor(max_workers=8) as pool:
    futures = [pool.submit(run, mode, implementation) for mode in modes for implementation in ['python', 'rust']]
    results = [future.result() for future in futures]
  for left, right in zip(results[::2], results[1::2], strict=True):
    assert left['value'] == right['value'], (left, right)
    assert left['requests'] == right['requests'], (left, right)
    if left['mode'] == 'progress':
      assert left['value'] == 'Asia/Seoul' and left['elapsed'] > 5 and right['elapsed'] > 5, (left, right)
    elif left['mode'].endswith(('late', 'stall')):
      assert left['value'] is None and 4.8 < left['elapsed'] < 6 and 4.8 < right['elapsed'] < 6, (left, right)
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'executions': len(results), 'results': results}, indent=2) + '\n')
  print(f'PASS: {len(results)} original urllib/native HTTP executions')


if __name__ == '__main__':
  main()
