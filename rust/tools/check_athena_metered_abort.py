#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from athena_fixture import daemon, private_environment, published, websocket_server, wait_for
from check_athena_transfers import device


def main():
  parser = argparse.ArgumentParser()
  for name in ['binary', 'ipc', 'output']:
    parser.add_argument(name, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  class Handler(BaseHTTPRequestHandler):
    def do_PUT(self):
      expected = int(self.headers['Content-Length'])
      row = {'expected': expected, 'received': 0}
      rows.append(row)
      digest = hashlib.sha256()
      while row['received'] < expected:
        data = self.rfile.read(min(65536, expected - row['received']))
        if not data:
          break
        row['received'] += len(data)
        digest.update(data)
        time.sleep(0.025)
      row['sha256'] = digest.hexdigest()
      if row['received'] == expected:
        self.send_response(200)
        self.send_header('Content-Length', '0')
        self.end_headers()
    def log_message(self, *_):
      return
  http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=http.serve_forever)
  thread.start()
  result = {'pass': False}
  try:
    with websocket_server() as (port, connected, _), private_environment(port) as env:
      payload = bytes(range(256)) * (48 * 1024)
      (env.logs / 'large').write_bytes(payload)
      with published(args.ipc, 'deviceState', env.env, device(False).to_bytes()) as packets, daemon(args.binary, args.output, env.env) as (process, pid):
        peer = connected.get(timeout=10)
        peer.rpc('uploadFileToUrl', ['large', f'http://127.0.0.1:{http.server_port}/large', {}])
        wait_for(lambda: rows and rows[0]['received'] > 0)
        time.sleep(1.2)
        packets[0] = device(True).to_bytes()
        wait_for(lambda: 'athena.upload_handler.abort' in (args.output / 'daemon.log').read_text(), 8)
        queue = peer.rpc('listUploadQueue')['result']
        assert queue and all(row['retry_count'] == 0 for row in queue), queue
        result['aborted_queue'] = queue
        wait_for(lambda: 'sha256' in rows[0], 8)
        assert 0 < rows[0]['received'] < rows[0]['expected'], rows
        packets[0] = device(False).to_bytes()
        wait_for(lambda: len(rows) == 2 and 'sha256' in rows[1], 22)
        assert rows[1]['received'] == len(payload) and rows[1]['sha256'] == hashlib.sha256(payload).hexdigest()
        wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 5)
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        result.update(returncode=process.returncode, **{'pass': True})
  finally:
    http.shutdown()
    thread.join(timeout=5)
    http.server_close()
    result['http_requests'] = rows
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual 12MiB upload interrupted by IPC metered transition,partial transfer,unchanged retry count,unmetered retry exact bytes and shutdown')


if __name__ == '__main__':
  main()
