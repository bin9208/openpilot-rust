#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import os
import signal
import threading
import time
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from openpilot.cereal import log
from athena_fixture import daemon, private_environment, published, wait_for, websocket_server
from athena_reference import source, ParamsStore
import requests


def device(metered):
  event = log.Event.new_message()
  event.logMonoTime = 123456789
  state = event.init('deviceState')
  state.networkMetered = metered
  state.networkType = 'cell4G' if metered else 'wifi'
  return event


@contextmanager
def server():
  records = []
  lock = threading.Lock()
  release = threading.Event()

  class Handler(BaseHTTPRequestHandler):
    def do_PUT(self):
      size = int(self.headers['Content-Length'])
      data = self.rfile.read(size)
      with lock:
        count = sum(row['path'] == self.path for row in records)
        records.append({'path': self.path, 'bytes': len(data), 'attempt': count})
      if self.path.startswith('/hold/'):
        release.wait(20)
      if self.path == '/disconnect' and count == 0:
        self.close_connection = True
        return
      self.send_response(200)
      self.send_header('Content-Length', '10' if self.path == '/truncated' else '0')
      self.end_headers()
      if self.path == '/truncated':
        self.wfile.write(b'x')
        self.wfile.flush()
        self.close_connection = True

    def log_message(self, *_):
      return

  http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=http.serve_forever)
  thread.start()
  try:
    yield http.server_port, records, release
  finally:
    release.set()
    http.shutdown()
    thread.join(timeout=5)
    http.server_close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('ipc_peer', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False}
  with websocket_server() as (port, connected, _), server() as (http_port, records, release), private_environment(port) as env:
    (env.logs / 'file').write_bytes(b'synthetic transfer' * 1000)
    with server() as (source_port, _, _):
      original = source(env.logs, ParamsStore())
      source_errors = []
      for endpoint in ['disconnect', 'truncated']:
        item = original['UploadItem'](str(env.logs / 'file'), f'http://127.0.0.1:{source_port}/{endpoint}', {}, int(time.time() * 1000), 'source-fixture')
        try:
          original['_do_upload'](item)
          raise AssertionError('source failure fixture unexpectedly succeeded')
        except (requests.exceptions.ConnectionError, requests.exceptions.ChunkedEncodingError) as error:
          source_errors.append(type(error).__name__)
      assert source_errors == ['ConnectionError', 'ChunkedEncodingError']
      result['unchanged_source_http_failure_classes'] = source_errors
    with published(args.ipc_peer, 'deviceState', env.env, device(False).to_bytes()) as packets:
      try:
        with daemon(args.binary, args.output, env.env, trace=True) as (process, pid):
          peer = connected.get(timeout=10)
          expected = device(False).to_dict()
          assert peer.rpc('getMessage', ['deviceState'])['result'] == expected
          result['actual_ipc_message'] = expected
          held = []
          for index in range(4):
            response = peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/hold/{index}', {}])
            held.append(response['result']['items'][0]['id'])
          wait_for(lambda: len(records) == 4)
          current_cancel = peer.rpc('cancelUpload', [held[0]])['result']
          assert current_cancel == {'success': 0, 'error': 'not found'}
          response = peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/cancelled?one=1', {}])
          queued_id = response['result']['items'][0]['id']
          duplicate = peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/cancelled?two=2', {}])['result']
          assert duplicate == {'enqueued': 0, 'items': []}
          assert peer.rpc('cancelUpload', [queued_id])['result'] == {'success': 1}
          assert all(item['id'] != queued_id for item in peer.rpc('listUploadQueue')['result'])
          result['cancel_current'] = current_cancel
          result['cancel_queued_id'] = queued_id
          release.set()
          wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 8)
          assert len(records) == 4
          packets[0] = device(True).to_bytes()
          time.sleep(0.15)
          assert peer.rpc('getMessage', ['deviceState'])['result']['deviceState']['networkMetered']
          peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/metered', {}])
          time.sleep(0.3)
          assert not any(row['path'] == '/metered' for row in records)
          pending = peer.rpc('listUploadQueue')['result']
          assert pending and all(item['retry_count'] == 0 for item in pending)
          result['metered_deferred'] = pending
          packets[0] = device(False).to_bytes()
          wait_for(lambda: any(row['path'] == '/metered' for row in records), 15)
          wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 5)
          peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/disconnect', {}])
          wait_for(lambda: sum(row['path'] == '/disconnect' for row in records) == 2, 15)
          wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 5)
          peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}/truncated', {}])
          wait_for(lambda: any(row['path'] == '/truncated' for row in records), 5)
          wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 5)
          cached = json.loads((env.params / 'AthenadUploadQueue').read_text())
          assert any(item['url'].endswith('/truncated') for item in cached)
          result['source_compatible_stale_cache_after_generic_body_error'] = cached
          (env.params / 'IsOnroad').write_text('1')
          wait_for(lambda: 'TCP_USER_TIMEOUT, [16000]' in (args.output / 'sockets.log').read_text(), 7)
          (env.params / 'IsOnroad').write_text('0')
          os.kill(pid, signal.SIGTERM)
          assert process.wait(timeout=8) == 0
          result['returncode'] = process.returncode
        trace = (args.output / 'sockets.log').read_text()
        assert 'IP_TOS, [32]' in trace
        assert all(value in trace for value in ['TCP_KEEPIDLE, [7]', 'TCP_KEEPINTVL, [7]', 'TCP_KEEPCNT, [2]'])
        result['pass'] = True
      finally:
        release.set()
        result['http_requests'] = records
        (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: actual native IPC,queued-only cancellation,query-insensitive duplicate URLs,metered deferral/no retry increment,recovery,disconnect retry,body-error cache,TOS and onroad TCP options')


if __name__ == '__main__':
  main()
