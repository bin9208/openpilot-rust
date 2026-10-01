import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import socket
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from athena_reference import source, ParamsStore


def main() -> None:
  if len(sys.argv) == 4 and sys.argv[1] == '--source':
    path = Path(sys.argv[2])
    original = source(path.parent, ParamsStore())
    item = original['UploadItem'](str(path), sys.argv[3], {}, 0, 'syscall-fixture')
    print(original['_do_upload'](item).status_code)
    return
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  fixture = args.fixture
  if fixture is None:
    fixture = args.output.resolve() / 'connect-fixture.so'
    subprocess.run(['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', '-fsanitize=undefined',
                    str(Path(__file__).with_name('athena_connect_fixture.c')), '-ldl', '-o', str(fixture)], check=True)
  payload = bytes(range(256)) * 128
  file = args.output.resolve() / 'payload'
  file.write_bytes(payload)
  received = []

  class Handler(BaseHTTPRequestHandler):
    def do_PUT(self):
      data = self.rfile.read(int(self.headers['Content-Length']))
      received.append({'path': self.path, 'sha256': hashlib.sha256(data).hexdigest()})
      self.send_response(200)
      self.send_header('Content-Length', '0')
      self.end_headers()

    def log_message(self, *_):
      return

  http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=http.serve_forever)
  thread.start()
  rows = []
  boundaries = []
  try:
    for kind in ['source', 'native']:
      url = f'http://127.0.0.1:{http.server_port}/{kind}'
      command = ([sys.executable, str(Path(__file__).resolve()), '--source'] if kind == 'source'
                 else [str(args.binary.resolve())]) + [str(file), url]
      env = dict(os.environ, LD_PRELOAD=str(fixture.resolve()), ATHENA_FIXTURE_PORT=str(http.server_port), NO_PROXY='*')
      process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=35)
      row = {'kind': kind, 'returncode': process.returncode, 'stdout': process.stdout, 'stderr': process.stderr}
      rows.append(row)
    started = time.monotonic()
    trace = args.output.resolve() / 'deadline.trace'
    process = subprocess.run(['strace', '-e', 'trace=ppoll', '-e', 'inject=ppoll:error=EINTR:when=1+',
                              '-o', str(trace), str(args.binary.resolve()), '--connect', str(http.server_port), '150'],
                             env=env, capture_output=True, text=True, timeout=3)
    elapsed = time.monotonic() - started
    boundaries.append({'case': 'interrupted_poll_deadline', 'returncode': process.returncode,
                       'stderr': process.stderr, 'elapsed_seconds': elapsed})
    assert process.returncode != 0 and 'TimedOut' in process.stderr, boundaries[-1]
    assert 0.1 <= elapsed < 2 and 'INJECTED' in trace.read_text(), boundaries[-1]
    with socket.socket() as reserved:
      reserved.bind(('127.0.0.1', 0))
      process = subprocess.run([str(args.binary.resolve()), '--connect', str(reserved.getsockname()[1]), '150'],
                               capture_output=True, text=True, timeout=3)
      boundaries.append({'case': 'refused', 'returncode': process.returncode, 'stderr': process.stderr})
      assert process.returncode != 0 and 'ConnectionRefused' in process.stderr, boundaries[-1]
  finally:
    http.shutdown()
    thread.join(timeout=5)
    http.server_close()
  result = {'results': rows, 'requests': received, 'boundaries': boundaries,
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  for row in rows:
    assert 'fixture: connected socket interrupted' in row['stderr'], row
    assert row['returncode'] == 0 and row['stdout'].strip() == '200', row
  assert received == [{'path': f'/{kind}', 'sha256': hashlib.sha256(payload).hexdigest()} for kind in ['source', 'native']]
  print('PASS: source/native full payload after interrupted pending TCP connection; interrupted poll deadline and refused connection')


if __name__ == '__main__':
  main()
