#!/usr/bin/env python3
"""Native assistance worker file/HTTP behavior with private files and loopback only."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import threading
import time


def scenario(binary, output, mode):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='qcom-assistance-') as temporary:
    root = Path(temporary)
    attempts = []
    data = b'x' * 110000 if mode == 'oversize' else b'synthetic assistance'
    class Handler(BaseHTTPRequestHandler):
      def do_GET(self):
        attempts.append(self.path)
        if mode == 'retry' and len(attempts) == 1:
          self.connection.shutdown(socket.SHUT_RDWR)
          self.connection.close()
          return
        self.send_response(404 if mode == 'http404' else 200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
      def log_message(self, *_):
        pass
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    path = root / 'assist'
    path.write_bytes(b'stale removed on worker startup')
    alternate = root / 'alternate'
    alternate.write_bytes(b'alternate replaces stale')
    config = {'at': {'path': str(root / 'unused-at'), 'lock': str(root / 'unused-lock')},
       'diagnostic': str(root / 'unused-diag'), 'nmea': str(root / 'unused-nmea'), 'root': str(root),
       'assistance': str(path), 'assistance_url': f'http://127.0.0.1:{server.server_port}/assist',
       'alternate': str(alternate) if mode == 'alternate' else None, 'mmcli': str(root / 'unused-mmcli'),
       'systemd': str(root / 'unused-systemd'), 'cold_start': False}
    process = None
    try:
      with (output / 'worker.log').open('w') as log_file:
        process = subprocess.Popen([binary, '--assistance-worker', json.dumps(config)], stdout=log_file, stderr=log_file)
        if mode == 'alternate':
          assert process.wait(timeout=3) == 0
          assert path.read_bytes() == alternate.read_bytes() and attempts == []
        else:
          end = time.monotonic() + 15
          while time.monotonic() < end:
            assert process.poll() is None
            if mode == 'oversize':
              if 'larger than expected' in (output / 'worker.log').read_text():
                break
            elif path.exists() and path.read_bytes() == data:
              break
            time.sleep(.02)
          else:
            raise AssertionError('assistance deadline exceeded')
          process.send_signal(signal.SIGUSR1)
          assert process.wait(timeout=3) == 0
          if mode == 'oversize':
            assert not path.exists()
            assert (root / 'assist.download').stat().st_size > 100000
          else:
            assert path.read_bytes() == data
            assert len(attempts) == (2 if mode == 'retry' else 1)
        (output / 'result.json').write_text(json.dumps({'mode': mode, 'attempts': attempts, 'returncode': process.returncode,
           'final_bytes': path.stat().st_size if path.exists() else None, 'pass': True}, indent=2) + '\n')
    finally:
      if process is not None:
        if process.poll() is None:
          process.kill()
        process.wait(timeout=3)
      server.shutdown()
      server.server_close()
      thread.join(timeout=2)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  for mode in ['alternate', 'http404', 'oversize', 'retry']:
    scenario(args.binary, args.output / mode, mode)
    print('PASS:', mode, flush=True)


if __name__ == '__main__':
  main()
