#!/usr/bin/env python3
import argparse
from collections import defaultdict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import pty
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import uuid

import zmq
from openpilot.cereal import log
from check_ublox import source_parser
from ublox_fixture import chunks, frame
from ublox_serial_child import Receiver


def wait_for(predicate, description):
  deadline = time.monotonic() + 15
  while time.monotonic() < deadline:
    if predicate():
      return
    time.sleep(0.01)
  raise AssertionError('timeout: ' + description)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  target = args.target.resolve() / 'debug'
  args.evidence.mkdir(parents=True, exist_ok=True)
  prefix = 'rust-probe-ublox-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  environment = os.environ | {'OPENPILOT_PREFIX': prefix}
  for name in ('ZMQ', 'CEREAL_FAKE'):
    environment.pop(name, None)
  requests = []
  assist = frame(0x1302, b'\x33')

  class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
      requests.append(self.path)
      self.send_response(200)
      self.send_header('Content-Length', str(len(assist)))
      self.end_headers()
      self.wfile.write(assist)

    def log_message(self, *args):
      return

  server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
  http_thread = threading.Thread(target=server.serve_forever)
  http_thread.start()
  context = zmq.Context()
  socket = context.socket(zmq.PULL)
  socket.bind('ipc:///tmp/logmessage' + prefix)
  packets, logs = [], []
  stopped = threading.Event()

  def logging():
    while not stopped.is_set():
      if socket.poll(50):
        data = socket.recv()
        logs.append(json.loads(data[data.find(b'{') :]))

  logger = threading.Thread(target=logging)
  logger.start()
  processes, handles, threads = [], [], []
  master, slave = pty.openpty()
  receiver = Receiver(master)
  try:
    with tempfile.TemporaryDirectory(prefix='ublox-daemon-') as directory:
      root = Path(directory)
      (root / 'dev').mkdir()
      (root / 'dev/ttyHS0').symlink_to(os.ttyname(slave))
      (root / 'TICI').touch()
      params = root / 'data/params' / prefix
      params.mkdir(parents=True)
      (params / 'AssistNowToken').write_text('fixture:+/,')
      for pin in (32, 33, 34):
        path = root / f'sys/class/gpio/gpio{pin}'
        path.mkdir(parents=True)
        (path / 'direction').write_text('in')
        (path / 'value').write_text('0')

      def start(name, arguments=(), output=None):
        error = (args.evidence / (Path(name).name + '.stderr')).open('w')
        handles.append(error)
        process = subprocess.Popen([target / name, *arguments], stdout=output, stderr=error, env=environment, text=True)
        processes.append(process)
        return process

      decoder = start('openpilot-ubloxd')
      peer = start('examples/ublox_peer', output=subprocess.PIPE)
      peer_ready = threading.Event()

      def collect():
        with (args.evidence / 'daemon-packets.jsonl').open('w') as capture:
          for line in peer.stdout:
            capture.write(line)
            capture.flush()
            row = json.loads(line)
            if row.get('ready'):
              peer_ready.set()
            else:
              with log.Event.from_bytes(bytes(row['packet'])) as event:
                packets.append((event.which(), event.to_dict(), bytes(event.ubloxRaw) if event.which() == 'ubloxRaw' else None))

      collector = threading.Thread(target=collect)
      collector.start()
      threads.append(collector)
      assert peer_ready.wait(5)
      pigeon = start('openpilot-pigeond', ['--root', str(root), '--assist-url', f'http://127.0.0.1:{server.server_port}/assist'])
      executables = {str(p.pid): os.readlink(f'/proc/{p.pid}/exe') for p in (decoder, peer, pigeon)}
      wait_for(lambda: sum(row.get('msg') == 'Pigeon GPS on!' for row in logs) == 1, 'first initialization')
      (root / 'dev/ttyHS0').unlink()
      stream = b''.join(bytes(row['bytes']) for row in chunks()[:-1])
      os.write(master, stream)
      wait_for(lambda: any(name == 'ubloxGnss' for name, _, _ in packets), 'GNSS publications')
      wait_for(lambda: sum(len(raw or b'') for _, _, raw in packets) >= len(stream), 'all raw bytes')
      os.write(master, b'\x00invalid')
      wait_for(lambda: sum(row.get('msg') == 'Pigeon GPS on!' for row in logs) == 2, 'leading-zero reinitialization')
      first_count = len(packets)
      os.write(master, stream)
      wait_for(lambda: sum(len(raw or b'') for _, _, raw in packets) >= 2 * len(stream), 'second raw stream')
      source = source_parser()
      expected = defaultdict(list)
      for name, event, raw in list(packets):
        if name != 'ubloxRaw':
          continue
        for packet in source.framer.add_data(event['logMonoTime'] * 1e-9, raw):
          try:
            result = source.parse_frame(packet)
          except Exception:
            continue
          if result:
            service, message = result
            value = message.to_dict()
            value.pop('logMonoTime')
            expected[service].append(value)
      wait_for(lambda: sum(name != 'ubloxRaw' for name, _, _ in packets) >= sum(map(len, expected.values())), 'decoder drain')
      actual = defaultdict(list)
      for name, value, _ in list(packets):
        if name == 'ubloxRaw':
          continue
        value = value.copy()
        value.pop('logMonoTime')
        actual[name].append(value)
      assert dict(actual) == dict(expected)
      assert first_count < len(packets)
      start_time = time.monotonic()
      pigeon.send_signal(signal.SIGINT)
      assert pigeon.wait(timeout=5) == 0
      wait_for(lambda: any(bytes(packet) == b'\xb5\x62\x06\x04\x04\x00\x00\x00\x08\x00\x16\x74' for packet in receiver.commands), 'controlled GNSS stop')
      stop_seconds = time.monotonic() - start_time
      gpio = {str(pin): (root / f'sys/class/gpio/gpio{pin}/value').read_text() for pin in (32, 33, 34)}
      assert gpio == {'32': '0', '33': '1', '34': '0'}
      assert requests == ['/assist?token=fixture:%2B%2F,&gnss=gps,glo&datatype=eph,alm,aux'] * 2, requests
      result = {
        'native_executables': executables,
        'publications': {name: len(rows) for name, rows in actual.items()},
        'source_native_messages_equal': True,
        'serial_initializations': 2,
        'existing_descriptor_reused_with_serial_path_removed': True,
        'stop_seconds': stop_seconds,
        'gpio': gpio,
        'http_requests': requests,
      }
      (args.evidence / 'daemon-results.json').write_text(json.dumps(result, indent=2) + '\n')
      (args.evidence / 'daemon-source-messages.json').write_text(json.dumps(expected, indent=2) + '\n')
      (args.evidence / 'daemon-native-messages.json').write_text(json.dumps(actual, indent=2) + '\n')
      print(json.dumps(result))
  finally:
    for process in reversed(processes):
      if process.poll() is None:
        process.send_signal(signal.SIGINT)
        try:
          process.wait(timeout=5)
        except subprocess.TimeoutExpired:
          process.kill()
          process.wait()
    for thread in threads:
      thread.join(timeout=2)
    receiver.close()
    stopped.set()
    logger.join(timeout=2)
    (args.evidence / 'daemon-serial-commands.json').write_text(json.dumps(receiver.commands, indent=2) + '\n')
    (args.evidence / 'daemon-logs.json').write_text(json.dumps(logs, indent=2) + '\n')
    for handle in handles:
      handle.close()
    socket.close(linger=0)
    context.term()
    server.shutdown()
    http_thread.join(timeout=2)
    server.server_close()
    os.close(master)
    os.close(slave)
    shutil.rmtree(shm)


if __name__ == '__main__':
  main()
