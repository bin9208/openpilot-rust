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


class RequestTrace:
  """Bounded sidecar only; no diagnostic fields enter comparison rows."""
  def __init__(self, limit=1024):
    self.limit = limit
    self._lock = threading.Lock()
    self._events = []
    self._dropped = 0
    self._sequence = 0

  def _append(self, request, event, fields):
    if len(self._events) >= self.limit:
      self._dropped += 1
      return
    self._events.append(dict(request, event=event, mono_ns=time.monotonic_ns(), wall_ns=time.time_ns(), **fields))

  def begin(self, method, path, client_port):
    parts = path.split('/')
    lane = parts[1] if len(parts) > 1 and parts[1] in ('source', 'native') else 'other'
    known = ('200', '201', '401', '403', '412', '500', 'expired', 'redirect301', 'redirect302',
             'redirect303', 'redirect307', 'redirect308', 'compressed')
    endpoint = parts[2] if len(parts) > 2 and parts[2] in known else 'other'
    with self._lock:
      self._sequence += 1
      request = dict(request=self._sequence, lane=lane, endpoint=endpoint,
                     redirect_hop=int(len(parts) == 4 and parts[3] == 'final'),
                     method=method if method in ('GET', 'PUT') else 'other', client_port=client_port)
      self._append(request, 'request_accepted', {})
    return request

  def record(self, request, event, **fields):
    with self._lock:
      self._append(request, event, fields)

  def snapshot(self):
    with self._lock:
      return dict(schema=1, limit=self.limit, dropped=self._dropped, events=[dict(event) for event in self._events])


def error_category(error):
  for kind, category in ((BrokenPipeError, 'broken_pipe'), (ConnectionResetError, 'connection_reset'),
                         (TimeoutError, 'timeout'), (OSError, 'io'), (ValueError, 'invalid_data')):
    if isinstance(error, kind):
      return category
  return 'other'


def upload_handler(rows, payload, trace):
  class Handler(BaseHTTPRequestHandler):
    def handle_request(self):
      request = trace.begin(self.command, self.path, self.client_address[1])
      phase = 'body_read'
      try:
        declared = int(self.headers.get('Content-Length', 0))
        data = self.rfile.read(declared)
        if len(data) == declared:
          trace.record(request, 'body_complete', declared_bytes=declared, received_bytes=len(data))
        else:
          trace.record(request, 'error', phase=phase, category='unexpected_eof',
                       declared_bytes=declared, received_bytes=len(data))
        phase = 'comparison_row'
        endpoint = self.path.rsplit('/', 1)[-1]
        status = int(endpoint) if endpoint.isdigit() else 200
        if endpoint.startswith('redirect'):
          status = int(endpoint.removeprefix('redirect'))
        rows.append({'path': self.path, 'method': self.command, 'status': status, 'length': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'decoded_sha256': hashlib.sha256(zstandard.ZstdDecompressor().decompress(data, max_output_size=len(payload))).hexdigest() if endpoint == 'compressed' else None})
        phase = 'response_write'
        self.send_response(status)
        if endpoint.startswith('redirect'):
          self.send_header('Location', self.path + '/final')
        self.send_header('Content-Length', '0')
        self.end_headers()
        self.wfile.flush()
        # Local socket writes completed, not proof the client received them.
        trace.record(request, 'response_flushed', status=status)
      except Exception as error:
        trace.record(request, 'error', phase=phase, category=error_category(error))
        raise
    do_PUT = handle_request
    do_GET = handle_request
    def log_message(self, *_):
      return
  return Handler


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  payload = bytes(range(256)) * 1000
  trace = RequestTrace()
  Handler = upload_handler(rows, payload, trace)
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
    (args.output / 'request-trace.json').write_text(json.dumps(trace.snapshot(), indent=2) + '\n')
    result['all_requests'] = rows
    (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual source/native HTTP status matrix,301/302/303/307/308 method/body redirects,zstd exact bytes,restored retry30 drop and expired no-request lifecycle')


if __name__ == '__main__':
  main()
