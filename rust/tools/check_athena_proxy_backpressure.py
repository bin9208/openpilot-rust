#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import signal
import socket
import struct
import threading
import time
from contextlib import contextmanager
from types import SimpleNamespace
from websockets.sync.server import serve
from websockets.exceptions import ConnectionClosed
from athena_fixture import daemon, private_environment, websocket_server
from athena_reference import source, ParamsStore

PAYLOAD = bytes(range(256)) * 32768
MARKER = b'reverse traffic during blocked output\x00'


@contextmanager
def remote():
  connections = queue.Queue()
  stop = threading.Event()
  def handler(ws):
    connections.put(ws)
    stop.wait(75)
  with serve(handler, '127.0.0.1', 0, max_size=None, max_queue=1, ping_interval=None, close_timeout=1) as server:
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
      yield server.socket.getsockname()[1], connections
    finally:
      stop.set()
      server.shutdown()
      thread.join(timeout=5)


@contextmanager
def implementation(kind, binary, output, port):
  with websocket_server() as (control_port, connected, _), private_environment(control_port) as env:
    if kind == 'native':
      with daemon(binary, output, env.env) as (process, pid):
        peer = connected.get(timeout=10)
        def start():
          response = peer.rpc('startLocalProxy', [f'ws://127.0.0.1:{port}/proxy', 22])
          assert response['result'] == {'success': 1}
          return response
        def stop():
          os.kill(pid, signal.SIGTERM)
          assert process.wait(timeout=8) == 0
        yield start, stop
        if process.poll() is None:
          assert peer.rpc('echo', ['proxy-complete'])['result'] == 'proxy-complete'
    else:
      params = ParamsStore()
      params.values['DongleId'] = 'synthetic146'
      scope = source(env.logs, params)
      scope['Api'] = lambda _: SimpleNamespace(get_token=lambda: 'owned-source-fixture')
      ended = threading.Event()
      threads = []
      def make_thread(*args, **kwargs):
        thread = threading.Thread(*args, **kwargs)
        threads.append(thread)
        return thread
      scope['threading'] = SimpleNamespace(Event=threading.Event, Thread=make_thread)
      try:
        yield lambda: scope['startLocalProxy'](ended, f'ws://127.0.0.1:{port}/proxy', 22), ended.set
      finally:
        ended.set()
        for thread in threads:
          thread.join(timeout=35)
        assert not any(thread.is_alive() for thread in threads)


def scenario(kind, name, binary, output):
  output.mkdir(parents=True, exist_ok=True)
  row = {'implementation': kind, 'scenario': name, 'pass': False, 'expected_bytes': len(PAYLOAD)}
  listener = socket.socket()
  listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
  listener.bind(('127.0.0.1', 22))
  listener.listen()
  listener.settimeout(8)
  writer = None
  local = None
  ws = None
  try:
    with remote() as (port, connections), implementation(kind, binary, output, port) as (start, stop):
      row['rpc'] = start()
      ws = connections.get(timeout=8)
      local, _ = listener.accept()
      local.settimeout(40)
      if name == 'down-close':
        received = bytearray()
        errors = []
        def receive():
          try:
            while data := local.recv(65536):
              received.extend(data)
          except OSError as error:
            errors.append(str(error))
        reader = threading.Thread(target=receive)
        reader.start()
        for offset in range(0, len(PAYLOAD), 4096):
          ws.send(PAYLOAD[offset:offset + 4096])
        ws.close()
        reader.join(timeout=30)
        assert not reader.is_alive()
        row.update(received_bytes=len(received), sha256=hashlib.sha256(received).hexdigest(), errors=errors)
        assert received == PAYLOAD and not errors, row
      else:
        sent = []
        def send():
          try:
            local.sendall(PAYLOAD)
            sent.append(len(PAYLOAD))
            local.shutdown(socket.SHUT_WR)
          except OSError as error:
            sent.append(str(error))
        writer = threading.Thread(target=send)
        writer.start()
        if name == 'up-eof':
          time.sleep(12)
          row['sender_finished_before_drain'] = not writer.is_alive()
          received = bytearray()
          error = None
          try:
            while True:
              received.extend(ws.recv(timeout=25))
          except ConnectionClosed as closed:
            error = str(closed)
            row['close_code'] = closed.rcvd.code if closed.rcvd is not None else None
          writer.join(timeout=5)
          row.update(sent_bytes=sent, received_bytes=len(received), sha256=hashlib.sha256(received).hexdigest(), close=error)
          assert received == PAYLOAD and sent == [len(PAYLOAD)], row
          assert not row['sender_finished_before_drain'], row
          assert row['close_code'] == 1000, row
        else:
          time.sleep(5)
          assert writer.is_alive(), 'sender was not backpressured'
          begin = time.monotonic()
          ws.send(MARKER)
          local.settimeout(3)
          actual = bytearray()
          while len(actual) < len(MARKER):
            actual.extend(local.recv(len(MARKER) - len(actual)))
          row['reverse_seconds'] = time.monotonic() - begin
          assert actual == MARKER
          begin = time.monotonic()
          if name == 'stalled-cancel':
            stop()
          else:
            ws.socket.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack('ii', 1, 0))
            ws.socket.close()
          local.settimeout(8)
          try:
            assert local.recv(1) == b''
          except ConnectionResetError:
            pass
          writer.join(timeout=5)
          assert not writer.is_alive()
          row.update(termination_seconds=time.monotonic() - begin, sent_bytes=sent)
          assert row['termination_seconds'] < 8, row
      row['pass'] = True
  finally:
    if local is not None:
      local.close()
    if ws is not None:
      ws.close()
    listener.close()
    if writer is not None:
      writer.join(timeout=5)
    (output / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
  return row


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--implementation', choices=['native', 'source', 'both'], default='both')
  parser.add_argument('--scenario', choices=['up-eof', 'down-close', 'stalled-cancel', 'peer-reset', 'all'], default='all')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  rows = []
  kinds = ['source', 'native'] if args.implementation == 'both' else [args.implementation]
  names = ['up-eof', 'down-close', 'stalled-cancel', 'peer-reset'] if args.scenario == 'all' else [args.scenario]
  try:
    for kind in kinds:
      for name in names:
        if kind == 'source' and name in ['stalled-cancel', 'peer-reset']:
          continue
        rows.append(scenario(kind, name, args.binary, args.output / f'{kind}-{name}'))
        print(f'PASS {kind} {name}', flush=True)
  finally:
    (args.output / 'summary.json').write_text(json.dumps({'cases': rows, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'network_namespace': os.readlink('/proc/self/ns/net')}, indent=2) + '\n')


if __name__ == '__main__':
  main()
