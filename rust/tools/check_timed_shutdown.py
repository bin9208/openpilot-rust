#!/usr/bin/env python3
"""Signal production timed during GPS sleep and progressing proxy HTTP without host changes."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from timed_fixtures import commands, environment


def worker(binary, output, mode, implementation, signal_name):
  output.mkdir(parents=True, exist_ok=True)
  request_started = threading.Event()
  peer_closed = threading.Event()
  shutdown = threading.Event()
  requests = []

  class Handler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def do_CONNECT(self):
      # Terminate the tunnel locally; never open the requested public host.
      self.send_response(200)
      self.end_headers()
      self.close_connection = False

    def log_message(self, *args):
      pass

    def do_GET(self):
      requests.append({'path': self.path, 'started': time.monotonic(), 'chunks': 0})
      body = b'{"status":"success","timezone":"missing","extra":"' + b'.' * 100 + b'"}'
      try:
        self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body[:1])
        self.wfile.flush()
        request_started.set()
        for byte in body[1:]:
          if shutdown.wait(0.1):
            break
          self.wfile.write(bytes([byte]))
          self.wfile.flush()
          requests[0]['chunks'] += 1
      except (BrokenPipeError, ConnectionResetError):
        peer_closed.set()

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  server_thread = threading.Thread(target=server.serve_forever, daemon=True)
  server_thread.start()
  process = None
  try:
    with tempfile.TemporaryDirectory(prefix='timed-stop-') as temporary, environment(Path(temporary)) as (config, params):
      config.update(wall=time.time_ns(), monotonic=None, live=True)
      if mode == 'gps':
        (params / 'TimezoneSource').write_text('app')
      proxy = f'http://127.0.0.1:{server.server_port}'
      os.environ.update(http_proxy=proxy, HTTP_PROXY=proxy, https_proxy=proxy, HTTPS_PROXY=proxy,
                        all_proxy=proxy, ALL_PROXY=proxy, no_proxy='', NO_PROXY='')
      prefix = os.environ['OPENPILOT_PREFIX']
      shm = Path('/dev/shm/msgq_' + prefix)
      shm.mkdir()
      publisher = msgq.pub_sock('gpsLocation', SERVICE_LIST['gpsLocation'].queue_size)
      receiver = msgq.sub_sock('clocks', conflate=False, timeout=5000, segment_size=SERVICE_LIST['clocks'].queue_size)
      args = [str(binary)] if implementation == 'rust' else [sys.executable, str(Path(__file__).with_name('timed_source_live.py'))]
      try:
        with (output / 'process.log').open('w') as stderr:
          process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=stderr, stderr=stderr, text=True)
          process.stdin.write(json.dumps(config) + '\n')
          process.stdin.close()
          publisher.wait_for_readers(timeout=5)
          packet = receiver.receive()
          assert packet is not None, ('missing clocks', process.poll())
          (output / 'clocks.capnp').write_bytes(packet)
          if mode == 'gps':
            event = log.Event.new_message()
            event.logMonoTime = time.monotonic_ns()
            event.valid = True
            gps = event.init('gpsLocation')
            gps.hasFix = True
            gps.unixTimestampMillis = time.time_ns() // 1000000 + 30000
            publisher.send(event.to_bytes())
            deadline = time.monotonic() + 5
            while not commands(config):
              assert time.monotonic() < deadline, ('missing command', process.poll())
              time.sleep(0.01)
          else:
            assert request_started.wait(5), ('proxy not contacted', process.poll())
            assert not shutdown.wait(6), 'unexpected shutdown'
            assert requests[0]['chunks'] > 40, requests
          assert process.poll() is None
          started = time.monotonic()
          process.send_signal(getattr(signal, signal_name))
          try:
            code = process.wait(timeout=12)
          except subprocess.TimeoutExpired:
            process.kill()
            code = process.wait(timeout=3)
          elapsed = time.monotonic() - started
          closed = mode == 'gps' or peer_closed.wait(2)
          result = {'mode': mode, 'implementation': implementation, 'signal': signal_name, 'elapsed': elapsed,
                    'exit_code': code, 'peer_closed': closed, 'requests': requests, 'commands': commands(config)}
          result['passed'] = elapsed < 2 and code == (0 if implementation == 'rust' else -getattr(signal, signal_name)) and closed
          (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
          return result
      finally:
        if process is not None and process.poll() is None:
          process.kill()
          process.wait(timeout=3)
        del publisher, receiver
        shutil.rmtree(shm)
  finally:
    shutdown.set()
    server.shutdown()
    server.server_close()
    server_thread.join()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--worker', nargs=3)
  args = parser.parse_args()
  if args.worker:
    worker(args.binary, args.output, *args.worker)
    return
  args.output.mkdir(parents=True, exist_ok=True)
  cases = [(mode, implementation, signal_name) for mode in ['gps', 'http']
           for implementation, signal_name in [('python', 'SIGINT'), ('rust', 'SIGINT'), ('rust', 'SIGTERM')]]

  def run(case):
    output = args.output / '-'.join(case)
    subprocess.run([sys.executable, __file__, '--binary', str(args.binary), '--output', str(output), '--worker', *case], check=True)
    return json.loads((output / 'result.json').read_text())

  with ThreadPoolExecutor(max_workers=3) as pool:
    results = list(pool.map(run, cases))
  passed = all(result['passed'] for result in results)
  (args.output / 'summary.json').write_text(json.dumps({'passed': passed, 'executions': len(results), 'results': results}, indent=2) + '\n')
  assert passed, results
  print('PASS: original SIGINT and native SIGINT/SIGTERM during GPS sleep and >5s progressing proxy HTTP; sockets close')


if __name__ == '__main__':
  main()
