#!/usr/bin/env python3
"""Compare fresh native upload processes and verify every multipart byte on loopback."""

from __future__ import annotations

import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading


def measure(binary: Path, root: Path, name: str, size: int) -> dict:
  result: dict = {}
  errors: list[str] = []
  pattern = bytes(range(256))

  class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
      pass

    def do_POST(self):
      try:
        assert self.path == '/tmux'
        content_type = self.headers['Content-Type']
        assert content_type.startswith('multipart/form-data; boundary=')
        boundary = content_type.split('boundary=', 1)[1]
        assert len(boundary) == 32 and all(c in '0123456789abcdef' for c in boundary)
        prefix = (
          f'--{boundary}\r\nContent-Disposition: form-data; name="reason"\r\n\r\nmeasurement\r\n'
          f'--{boundary}\r\nContent-Disposition: form-data; name="files[0]"; filename="tmux.log"\r\n'
          'Content-Type: text/plain\r\n\r\n'
        ).encode()
        suffix = f'\r\n--{boundary}--\r\n'.encode()
        assert int(self.headers['Content-Length']) == len(prefix) + size + len(suffix)
        assert self.rfile.read(len(prefix)) == prefix
        digest = hashlib.sha256()
        remaining = size
        while remaining:
          count = min(65536, remaining)
          chunk = self.rfile.read(count)
          assert chunk == (pattern * ((count + 255) // 256))[:count]
          digest.update(chunk)
          remaining -= count
        assert self.rfile.read(len(suffix)) == suffix
        result.update(payload_bytes=size, payload_sha256=digest.hexdigest(), content_length=len(prefix) + size + len(suffix))
        body = b'{"ok":true}'
        self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
      except (AssertionError, OSError, ValueError, TypeError) as error:
        errors.append(repr(error))
        self.close_connection = True

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  server.daemon_threads = True
  server.timeout = 1
  thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': 0.01}, daemon=True)
  thread.start()
  rss = root / f'{name}.rss-kib'
  request = {
    'op': 'tmux', 'url': f'http://127.0.0.1:{server.server_port}/tmux',
    'headers': {}, 'payload': {'reason': 'measurement'}, 'tmux_path': str(root / 'tmux.log'),
  }
  try:
    child = subprocess.Popen(
      ['/usr/bin/time', '-f', '%M', '-o', str(rss), str(binary)],
      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
      text=True, start_new_session=True,
    )
    try:
      stdout, stderr = child.communicate(json.dumps(request) + '\n', timeout=90)
    except subprocess.TimeoutExpired:
      # The unreaped owned session leader retains the group identity for cleanup.
      try:
        os.killpg(child.pid, signal.SIGKILL)
      except ProcessLookupError:
        pass
      child.communicate()
      raise
    (root / f'{name}.stdout').write_text(stdout)
    (root / f'{name}.stderr').write_text(stderr)
    assert child.returncode == 0, (name, child.returncode, stderr)
    response = json.loads(stdout)
    assert response['result']['status'] == 200, response
    assert not errors and result, (errors, result)
    result['peak_rss_kib'] = int(rss.read_text().strip())
    with binary.open('rb') as stream:
      result['binary_sha256'] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return result
  finally:
    server.shutdown()
    server.server_close()
    thread.join(timeout=5)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--baseline', type=Path, required=True)
  parser.add_argument('--candidate', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  size = 64 * 1024 * 1024
  with tempfile.TemporaryDirectory(prefix='multipart-351-') as temporary:
    root = Path(temporary)
    with (root / 'tmux.log').open('wb') as stream:
      for _ in range(1024):
        stream.write(bytes(range(256)) * 256)
    baselines, candidates = [], []
    for index in range(3):
      baselines.append(measure(args.baseline.resolve(), root, f'baseline-{index}', size))
      candidates.append(measure(args.candidate.resolve(), root, f'candidate-{index}', size))
    for result in baselines + candidates:
      assert result['payload_sha256'] == baselines[0]['payload_sha256']
      assert result['content_length'] == baselines[0]['content_length']
    # Removing a simultaneous 64 MiB file buffer must produce a material result,
    # not a few pages of RSS noise. Compare the least favorable of three pairs.
    minimum_reduction_kib = size // (2 * 1024)
    observed_reduction_kib = min(x['peak_rss_kib'] for x in baselines) - max(x['peak_rss_kib'] for x in candidates)
    assert observed_reduction_kib >= minimum_reduction_kib, (baselines, candidates)
    for path in root.iterdir():
      if path.suffix in {'.stdout', '.stderr', '.rss-kib'}:
        (args.output / path.name).write_bytes(path.read_bytes())
    report = {
      'baselines': baselines, 'candidates': candidates,
      'minimum_reduction_kib': minimum_reduction_kib, 'observed_reduction_kib': observed_reduction_kib,
      'scope': 'fresh host process peak RSS; exact loopback bytes; no device or CPU measurement',
    }
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
  main()
