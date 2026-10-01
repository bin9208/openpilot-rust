#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path
import queue
import signal
import os
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from contextlib import contextmanager
import jwt
from athena_fixture import daemon, private_environment, wait_for, websocket_server


@contextmanager
def uploads_server():
  uploads = []
  counts = {}

  class Handler(BaseHTTPRequestHandler):
    def do_PUT(self):
      length = int(self.headers['Content-Length'])
      body = self.rfile.read(length)
      counts[self.path] = counts.get(self.path, 0) + 1
      status = 500 if self.path == '/retry' and counts[self.path] == 1 else 201
      uploads.append({'path': self.path, 'size': len(body), 'sha256': hashlib.sha256(body).hexdigest(), 'status': status, 'headers': dict(self.headers)})
      self.send_response(status)
      self.send_header('Content-Length', '0')
      self.end_headers()

    def log_message(self, *_):
      return

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=server.serve_forever)
  thread.start()
  try:
    yield server.server_port, uploads
  finally:
    server.shutdown()
    thread.join(timeout=5)
    server.server_close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('ipc_peer', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False}
  with websocket_server() as (port, connected, peers), uploads_server() as (http_port, uploads), private_environment(port) as env:
    payload = b'synthetic upload payload\x00' * 2000
    (env.logs / 'file').write_bytes(payload)
    (env.home / 'stats/fixture-stat').write_text('synthetic statistics\r\n')
    try:
      with daemon(args.binary, args.output, env.env, trace=True) as (process, pid):
        peer = connected.get(timeout=10)
        cookie = peer.headers['cookie']
        claims = jwt.decode(cookie.removeprefix('jwt='), env.public, algorithms=['ES256'])
        assert claims['identity'] == 'synthetic146' and claims['exp'] - claims['iat'] == 3600
        assert peer.path == '/ws/v2/synthetic146'
        result['native_executable'] = str(Path(f'/proc/{pid}/exe').resolve())
        result['authenticated'] = claims
        assert peer.rpc('echo', ['hello'])['result'] == 'hello'
        long = '한글😀' * 5000
        assert peer.rpc('echo', [long])['result'] == long
        large = peer.messages[-1]['chunks']
        assert len(large) > 1 and all(length == 4096 for length in large[:-1]), large
        result['large_reply_chunks'] = large
        ping = peer.socket.ping(b'fixture-ping')
        assert ping.wait(5)
        result['ping_ns'] = int(wait_for(lambda: (env.params / 'LastAthenaPingTime').read_text() if (env.params / 'LastAthenaPingTime').exists() else None))
        assert result['ping_ns'] > 0
        assert peer.rpc('getPublicKey')['result'] == env.public.decode()
        assert peer.rpc('takeSnapshot')['result'] == {'jpegBack': None, 'jpegFront': None}
        assert peer.rpc('getVersion')['result']['version'] == '1.2.3'
        assert peer.rpc('setRouteViewed', ['fixture-route'])['result']['success'] == 1
        assert (env.params / 'AthenadRecentlyViewedRoutes').read_text() == 'fixture-route'
        for path in ['/normal', '/retry']:
          reply = peer.rpc('uploadFileToUrl', ['file', f'http://127.0.0.1:{http_port}{path}', {'X-Fixture': '146'}])
          assert reply['result']['enqueued'] == 1, reply
        wait_for(lambda: len(uploads) >= 3, 15)
        assert all(row['sha256'] == hashlib.sha256(payload).hexdigest() for row in uploads)
        wait_for(lambda: peer.rpc('listUploadQueue')['result'] == [], 15)
        wait_for(lambda: not (env.home / 'stats/fixture-stat').exists())
        assert any(message['packet'].get('method') == 'storeStats' for message in peer.messages)
        result['upload_queue_cache'] = json.loads((env.params / 'AthenadUploadQueue').read_text())
        assert result['upload_queue_cache'] == []
        peer.socket.close()
        peer = connected.get(timeout=8)
        assert peer.rpc('echo', ['reconnected'])['result'] == 'reconnected'
        result['connections'] = len(peers)
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        result['returncode'] = process.returncode
      result['uploads'] = uploads
      native_log = (args.output / 'daemon.log').read_text()
      starts = [json.loads(line) for line in native_log.splitlines() if line.startswith('{') and 'athena.upload_handler.upload_start' in line]
      assert sorted(row['retry_count'] for row in starts) == [0, 0, 1], starts
      assert 'athena.upload_handler.timeout' not in native_log
      result['upload_starts'] = starts
      result['pass'] = True
    finally:
      result['messages'] = [peer.messages for peer in peers]
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: authenticated native WS RPC/fragments/ping/reconnect, actual HTTP retry/upload, stats, Params and SIGTERM')


if __name__ == '__main__':
  main()
