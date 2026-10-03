#!/usr/bin/env python3
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid


def scenario(args, side, mode):
  output = args.output / side / mode
  output.mkdir(parents=True)
  captures, statuses = [], []
  reports = 0
  release_response = threading.Event()

  class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
      pass
    def do_POST(self):
      nonlocal reports
      body = self.rfile.read(int(self.headers['Content-Length']))
      if self.path == '/report':
        reports += 1
      code = 503 if mode == 'continuous' and self.path == '/report' and reports == 1 else 200
      captures.append({'path': self.path, 'body': body.decode(), 'status': code})
      if mode == 'stop-http':
        assert release_response.wait(15)
      self.send_response(code)
      self.send_header('Content-Length', '2')
      self.end_headers()
      try:
        self.wfile.write(b'ok')
      except (BrokenPipeError, ConnectionResetError):
        pass

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  thread = threading.Thread(target=server.serve_forever)
  thread.start()
  process = None
  try:
    with tempfile.TemporaryDirectory(prefix='cweb-qa-') as temporary:
      root = Path(temporary)
      ip = root / 'address'
      ip.write_text('' if mode == 'no-ip' else '10.1.2.3')
      prefix = 'cweb-' + uuid.uuid4().hex[:12]
      params = root / 'params' / prefix
      params.mkdir(parents=True)
      values = {'DongleId': list(b' Unknown \n'), 'HardwareSerial': list(' 한글-fixture '.encode())}
      if mode == 'invalid-id':
        values['DongleId'] = [255, 254]
      (root / 'values.json').write_text(json.dumps(values))
      for key, value in values.items():
        (params / key).write_bytes(bytes(value))
      url = f'http://127.0.0.1:{server.server_port}/report'
      options = ['--url', url, '--heartbeat-interval', '5', '--debounce', '0', '--interval', '0.02']
      if mode != 'continuous':
        options += ['--once']
      if mode == 'dry-run':
        options += ['--dry-run']
      environment = os.environ | {'PARAMS_ROOT': str(root / 'params'), 'OPENPILOT_PREFIX': prefix, 'CWEB_FIXTURE_IP': str(ip), 'NO_PROXY': '127.0.0.1'}
      command = ([str(args.binary), '--fixture-ip', str(ip)] if side == 'native' else
                 [sys.executable, str(Path(__file__).with_name('cweb_source_peer.py')), str(args.binding), str(root)]) + options
      with (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1)
        executable = str(Path(f'/proc/{process.pid}/exe').resolve())
        if side == 'native':
          assert executable == str(args.binary)
          assert 'libpython' not in Path(f'/proc/{process.pid}/maps').read_text()

        def receive(timeout=15):
          deadline = time.monotonic() + timeout
          while time.monotonic() < deadline:
            if select.select([process.stdout], [], [], .2)[0]:
              line = process.stdout.readline()
              assert line, (process.poll(), (output / 'stderr.log').read_text())
              if line.startswith('[cweb_push] {'):
                value = json.loads(line.removeprefix('[cweb_push] '))
                value.pop('ts')
                statuses.append(value)
                return value
          raise AssertionError(('status timeout', mode, statuses, (output / 'stderr.log').read_text()))

        def wait_for(state):
          while receive()['state'] != state:
            pass

        if mode == 'stop-http':
          wait_for('ip_candidate')
          deadline = time.monotonic() + 5
          while not captures:
            assert process.poll() is None and time.monotonic() < deadline
            time.sleep(.01)
          started = time.monotonic()
          process.send_signal(signal.SIGTERM)
          code = process.wait(timeout=2)
          shutdown = time.monotonic() - started
          assert shutdown < 1 and code == (0 if side == 'native' else -signal.SIGTERM)
        elif mode == 'continuous':
          wait_for('report_failed')
          wait_for('reported')
          wait_for('heartbeat')
          ip.write_text('10.2.3.4')
          wait_for('reported')
          ip.write_text('')
          wait_for('no_ip')
          started = time.monotonic()
          process.send_signal(signal.SIGTERM)
          code = process.wait(timeout=3)
          assert code == (0 if side == 'native' else -signal.SIGTERM)
          shutdown = time.monotonic() - started
        else:
          wait_for('once_no_report' if mode == 'no-ip' else 'dry_run' if mode == 'dry-run' else 'reported')
          assert process.wait(timeout=3) == 0
          shutdown = None
        if mode in ['dry-run', 'no-ip']:
          assert not captures
        else:
          assert all(json.loads(row['body'])['deviceId'] == '한글-fixture' for row in captures)
        normalized = [row for row in statuses if row['state'] not in ['idle', 'no_ip']]
        if mode == 'no-ip':
          assert any(row['state'] == 'no_ip' for row in statuses)
        result = {'statuses': normalized, 'requests': captures}
        (output / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2))
        execution = {'argv': command, 'executable': executable, 'exit': process.returncode, 'shutdown': shutdown}
        (output / 'execution.json').write_text(json.dumps(execution, indent=2))
        return result
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait()
    release_response.set()
    server.shutdown()
    server.server_close()
    thread.join()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--modes', nargs='+', default=['once', 'dry-run', 'no-ip', 'invalid-id', 'continuous', 'stop-http'])
  args = parser.parse_args()
  args.binary, args.binding, args.output = [path.resolve() for path in (args.binary, args.binding, args.output)]
  for mode in args.modes:
    source = scenario(args, 'source', mode)
    native = scenario(args, 'native', mode)
    assert source == native, (mode, source, native)
    print('PASS', mode, flush=True)
  (args.output / 'comparison.json').write_text(json.dumps({'scenarios': len(args.modes), 'source_native_equal': True}))


if __name__ == '__main__':
  main()
