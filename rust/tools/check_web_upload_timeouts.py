# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp==3.13.3", "anyio==4.12.1", "requests==2.34.2"]
# ///
# Run: uv run rust/tools/check_web_upload_timeouts.py --binary rust/target/debug/examples/web_upload_trace --output /tmp/web-upload-timeouts
"""Real loopback stalls exercise the source's unscaled timeout constants."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import socketserver
import subprocess
import tempfile
import threading
import time
from functools import partial
from pathlib import Path

from check_web_upload import original
from web_upload_fixture import Fixture, Json, server_for


def stalled_tls(fixture: Fixture) -> socketserver.ThreadingTCPServer:
  class Handler(socketserver.BaseRequestHandler):
    def handle(self) -> None:
      hello = self.request.recv(8192)
      fixture.record({'method': 'TLS', 'path': '', 'headers': {}, 'chunks': [], 'complete': True}, hello)
      threading.Event().wait(21)

  return socketserver.ThreadingTCPServer(('127.0.0.1', 0), Handler)


def execute(binary: Path, directory: Path, name: str, language: str, scenario: dict[str, Json], plan: dict[str, Json]) -> dict[str, Json]:
  fixture = Fixture(directory, plans=[plan], prefix=f'{name}-{language}')
  server = stalled_tls(fixture) if plan.get('tls_stall') else server_for(fixture)
  thread = threading.Thread(target=partial(server.serve_forever, poll_interval=0.01), daemon=True)
  thread.start()
  scheme = 'https' if plan.get('tls_stall') else 'http'
  command = scenario | {'base': f'{scheme}://127.0.0.1:{server.server_address[1]}'}
  if command['op'] == 'tmux':
    command['url'] = command['base'] + '/tmux'
  started = time.monotonic()
  try:
    if language == 'python':
      result = original(command)
    else:
      invocation = subprocess.run([str(binary)], input=json.dumps(command) + '\n', text=True, capture_output=True, check=True, timeout=410)
      result = json.loads(invocation.stdout)
    elapsed = time.monotonic() - started
  finally:
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)
  record = {
    'name': name,
    'language': language,
    'elapsed': elapsed,
    'result': result,
    'requests': fixture.captures,
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'command': command,
  }
  (directory / f'{name}-{language}.json').write_text(json.dumps(record, indent=2))
  if name == 'file-read-timeout':
    assert 350 < elapsed < 390, record
    assert 'error' in result and len(fixture.captures) == 2, record
  elif name == 'file-connect-timeout':
    assert 38 < elapsed < 46 and 'error' in result and len(fixture.captures) == 2, record
  elif name == 'session-tls-total':
    assert 11 < elapsed < 16 and 'error' in result, record
  elif name == 'tmux-socket-timeout':
    assert 28 < elapsed < 35 and 'error' in result, record
  elif name == 'session-total-timeout':
    assert 11 < elapsed < 16 and 'error' in result, record
  elif name == 'sync-progressing':
    assert 13 < elapsed < 20 and result.get('result') == 'synthetic-session', record
  elif name == 'file-progressing':
    assert 13 < elapsed < 20 and result.get('result') is True, record
  else:
    raise AssertionError(name)
  return record


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='web-upload-timeouts-') as temporary:
    path = Path(temporary) / 'rlog.zst'
    path.write_bytes(b'synthetic-log')
    session = {'op': 'session', 'metadata': {'deviceId': 'synthetic'}, 'purpose': 'dashcam', 'sync': False}
    folder = {'op': 'folder', 'folder': temporary, 'token': 'synthetic', 'directory': 'device', 'remote_path': 'route', 'filenames': ['rlog.zst']}
    tmux = {'op': 'tmux', 'headers': {}, 'payload': {}, 'tmux_path': str(path), 'settings_path': None}
    scenarios = [
      ('file-read-timeout', folder, {'delay_headers': 181}),
      ('file-connect-timeout', folder, {'tls_stall': True}),
      ('session-tls-total', session, {'tls_stall': True}),
      ('tmux-socket-timeout', tmux, {'delay_headers': 31}),
      ('session-total-timeout', session, {'delay_body': 4.6, 'response_pieces': 3}),
      ('sync-progressing', session | {'sync': True}, {'delay_body': 4.6, 'response_pieces': 3}),
      ('file-progressing', folder, {'delay_body': 4.6, 'response_pieces': 3}),
    ]
    with concurrent.futures.ThreadPoolExecutor(max_workers=14) as pool:
      futures = [
        pool.submit(execute, args.binary.resolve(), args.output, name, language, scenario, plan)
        for name, scenario, plan in scenarios
        for language in ['python', 'rust']
      ]
      records = [future.result() for future in concurrent.futures.as_completed(futures)]
    (args.output / 'report.json').write_text(json.dumps({'passed': len(records), 'cases': records}, indent=2))
    print(json.dumps({'passed': len(records), 'report': str(args.output / 'report.json')}))


if __name__ == '__main__':
  main()
