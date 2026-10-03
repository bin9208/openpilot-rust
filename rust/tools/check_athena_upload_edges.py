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
import zstandard
from athena_fixture import daemon, private_environment, websocket_server, wait_for
from athena_reference import source, ParamsStore


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  payload = bytes(range(256)) * 1000
  class Handler(BaseHTTPRequestHandler):
    def handle_request(self):
      data = self.rfile.read(int(self.headers.get('Content-Length', 0)))
      endpoint = self.path.rsplit('/', 1)[-1]
      status = int(endpoint) if endpoint.isdigit() else 200
      if endpoint.startswith('redirect'):
        status = int(endpoint.removeprefix('redirect'))
      rows.append({'path': self.path, 'method': self.command, 'status': status, 'length': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'decoded_sha256': hashlib.sha256(zstandard.ZstdDecompressor().decompress(data, max_output_size=len(payload))).hexdigest() if endpoint == 'compressed' else None})
      self.send_response(status)
      if endpoint.startswith('redirect'):
        self.send_header('Location', self.path + '/final')
      self.send_header('Content-Length', '0')
      self.end_headers()
    do_PUT = handle_request
    do_GET = handle_request
    def log_message(self, *_):
      return
  http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=http.serve_forever)
  thread.start()
  result = {'pass': False}
  try:
    with websocket_server() as (port, connected, _), private_environment(port) as env:
      (env.logs / 'file').write_bytes(payload)
      original = source(env.logs, ParamsStore())
      paths = ['200', '201', '401', '403', '412', 'redirect301', 'redirect302', 'redirect303', 'redirect307', 'redirect308', 'compressed']
      for path in paths:
        item = original['UploadItem'](str(env.logs / ('file.zst' if path == 'compressed' else 'file')), f'http://127.0.0.1:{http.server_port}/source/{path}', {}, int(time.time() * 1000), 'source-fixture')
        response = original['_do_upload'](item)
        assert response.status_code in [200, 201, 401, 403, 412]
      now = int(time.time() * 1000)
      cached = [dict(path=str(env.logs / 'file'), url=f'http://127.0.0.1:{http.server_port}/native/{path}', headers={}, created_at=created, id=path, retry_count=retries, current=False, progress=0, allow_cellular=True, priority=0) for path, created, retries in [('expired', now - 32 * 86400000, 0), ('500', now, 30)]]
      (env.params / 'AthenadUploadQueue').write_text(json.dumps(cached))
      with daemon(args.binary, args.output, env.env) as (process, pid):
        peer = connected.get(timeout=10)
        for path in paths:
          response = peer.rpc('uploadFileToUrl', ['file.zst' if path == 'compressed' else 'file', f'http://127.0.0.1:{http.server_port}/native/{path}', {}])
          assert response['result']['enqueued'] == 1, response
        wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 15)
        source_rows = [dict(row, path=row['path'].replace('/source/', '/')) for row in rows if row['path'].startswith('/source/')]
        native_rows = [dict(row, path=row['path'].replace('/native/', '/')) for row in rows if row['path'].startswith('/native/') and row['path'] != '/native/500']
        assert sorted(source_rows, key=lambda row: row['path']) == sorted(native_rows, key=lambda row: row['path']), (source_rows, native_rows)
        assert sum(row['path'] == '/native/500' for row in rows) == 1
        assert not any(row['path'] == '/native/expired' for row in rows)
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        result.update(source_requests=source_rows, native_requests=native_rows, max_retry_single_attempt=True, expired_no_request=True, returncode=process.returncode, **{'pass': True})
  finally:
    http.shutdown()
    thread.join(timeout=5)
    http.server_close()
    result['all_requests'] = rows
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual source/native HTTP status matrix,301/302/303/307/308 method/body redirects,zstd exact bytes,restored retry30 drop and expired no-request lifecycle')


if __name__ == '__main__':
  main()
