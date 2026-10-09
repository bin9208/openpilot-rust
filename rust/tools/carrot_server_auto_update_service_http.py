from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import threading
import time

from carrot_server_auto_update_service import ROOT, TOOLS


class Peer(BaseHTTPRequestHandler):
  protocol_version = 'HTTP/1.1'
  records: list[dict] = []

  def log_message(self, *_: object) -> None:
    pass

  def respond(self) -> None:
    body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
    self.records.append({'method': self.command, 'path': self.path, 'body': body.decode(),
      'content_type': self.headers.get('Content-Type'), 'user_agent': self.headers.get('User-Agent')})
    if self.path == '/slow':
      time.sleep(4.3)
    status = 503 if self.path in {'/error', '/short-error'} else 303 if self.path == '/redirect' else 307 if self.path == '/reject' else 200
    data = b'abc' if self.path.startswith('/short') else b'owned\xff' if self.path == '/ok' else b'owned'
    self.send_response(status)
    if self.path in {'/redirect', '/reject'}:
      self.send_header('Location', '/ok')
    if self.path == '/chunk-short':
      self.send_header('Transfer-Encoding', 'chunked')
    else:
      self.send_header('Content-Length', '9' if self.path.startswith('/short') else str(len(data)))
    self.end_headers()
    try:
      self.wfile.write(b'3\r\nabc\r\n' if self.path == '/chunk-short' else data)
    except BrokenPipeError:
      pass
    self.close_connection = True

  do_POST = respond
  do_GET = respond


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--case', action='append', default=[])
  parser.add_argument('--transport-only', action='store_true')
  parser.add_argument('--post-case', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  peer = ThreadingHTTPServer(('127.0.0.1', 0), Peer)
  worker = threading.Thread(target=peer.serve_forever)
  worker.start()
  pairs = []
  notifications = []
  try:
    address = f'http://127.0.0.1:{peer.server_port}'
    command = [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_service.py'), str(args.output / 'policy'),
      '--binary', str(args.binary), '--notify-url', address + '/notify']
    for name in args.case:
      command.extend(['--case', name])
    if not args.transport_only:
      invocation = subprocess.run(command, capture_output=True, text=True, timeout=45)
      (args.output / 'policy-invocation.json').write_text(json.dumps({
        'argv': command, 'exit': invocation.returncode, 'stdout': invocation.stdout, 'stderr': invocation.stderr,
      }, indent=2))
      assert invocation.returncode == 0, invocation.stderr
      notifications = list(Peer.records)
      assert len(notifications) == 2 and notifications[0] == notifications[1], notifications
      payload = json.loads(notifications[0]['body'])
      assert payload['count'] == 14 and len(payload['commits']) == 10 and payload['deviceId'] == 'owned-기기' and payload['token'] == 'owned-token', payload
      assert notifications[0]['user_agent'] == 'openpilot-cweb-push/1'
      assert '\\u' not in notifications[0]['body'] and ' ' not in notifications[0]['body'].replace(' | subject', '').replace('알림 ', '')
    for path in ['/ok', '/error', '/short', '/short-error', '/redirect', '/reject', '/chunk-short', '/slow']:
      if args.post_case and path not in args.post_case:
        continue
      outcomes = []
      calls = []
      for kind, program in [('source', [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_service_source.py')]), ('native', [str(args.binary)])]:
        root = args.output / path[1:] / kind
        root.mkdir(parents=True)
        config = {'mode': 'post', 'source': str(ROOT), 'repository': str(root), 'state': str(root / 'state'),
          'params': str(root / 'params'), 'lock': str(root / 'lock'),
          'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'), 'head': address + path, 'steps': [{'a': '기기😀', 'b': 'owned'}]}
        before = len(Peer.records)
        start = time.monotonic()
        run = subprocess.run(program, input=json.dumps(config) + '\n', capture_output=True, text=True, timeout=8)
        row = {'argv': program, 'exit': run.returncode, 'seconds': time.monotonic() - start, 'stdout': run.stdout, 'stderr': run.stderr}
        (root / 'invocation.json').write_text(json.dumps(row, indent=2))
        assert run.returncode == 0, row
        outcomes.append(json.loads(run.stdout.splitlines()[-1]))
        calls.append(Peer.records[before:])
      row = {'name': path, 'source': outcomes[0], 'native': outcomes[1], 'source_requests': calls[0], 'native_requests': calls[1],
        'equal': outcomes[0] == outcomes[1] and calls[0] == calls[1]}
      pairs.append(row)
      print(json.dumps({'name': path, 'equal': row['equal']}), flush=True)
  finally:
    (args.output / 'requests.json').write_text(json.dumps(Peer.records, indent=2, ensure_ascii=False))
    peer.shutdown()
    peer.server_close()
    worker.join()
  result = {'notifications': notifications, 'pairs': pairs,
    'differences': [row['name'] for row in pairs if not row['equal']], 'peer_stopped': not worker.is_alive()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2, ensure_ascii=False))
  assert not result['differences'], result['differences']


if __name__ == '__main__':
  main()
