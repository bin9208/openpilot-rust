#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import signal
import socket
import threading
from contextlib import contextmanager
import jwt
from websockets.sync.server import serve
from websockets.exceptions import ConnectionClosed
from athena_fixture import daemon, private_environment, websocket_server


@contextmanager
def local_target():
  listener = socket.socket()
  listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
  listener.bind(('127.0.0.1', 22))
  listener.listen()
  listener.settimeout(0.1)
  stop = threading.Event()
  records = []

  def accept():
    while not stop.is_set():
      try:
        connection, _ = listener.accept()
      except TimeoutError:
        continue
      received = bytearray()
      with connection:
        connection.settimeout(1)
        while not stop.is_set():
          try:
            data = connection.recv(513)
          except TimeoutError:
            continue
          if not data:
            break
          received.extend(data)
          for offset in range(0, len(data), 127):
            connection.sendall(data[offset:offset + 127])
      records.append({'bytes': len(received), 'sha256': hashlib.sha256(received).hexdigest()})

  thread = threading.Thread(target=accept)
  thread.start()
  try:
    yield records
  finally:
    stop.set()
    thread.join(timeout=5)
    listener.close()
    assert not thread.is_alive()


@contextmanager
def remote_proxy():
  connected = queue.Queue()
  stop = threading.Event()
  sockets = []

  def handler(connection):
    sockets.append(connection)
    connected.put(connection)
    stop.wait()

  with serve(handler, '127.0.0.1', 0, max_size=None, ping_interval=None) as server:
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
      yield server.socket.getsockname()[1], connected
    finally:
      stop.set()
      for connection in sockets:
        connection.close()
      server.shutdown()
      thread.join(timeout=5)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False, 'network_namespace': os.readlink('/proc/self/ns/net')}
  with local_target() as targets, remote_proxy() as (remote_port, remote_connected), websocket_server() as (port, connected, _), private_environment(port) as env:
    try:
      with daemon(args.binary, args.output, env.env, trace=True) as (process, pid):
        peer = connected.get(timeout=10)
        denied = peer.rpc('startLocalProxy', [f'ws://127.0.0.1:{remote_port}/proxy', 23])
        assert denied['error']['data']['message'] == 'Requested local port not whitelisted'
        result['denied'] = denied
        records = []
        for requested in [22, 8022]:
          assert peer.rpc('startLocalProxy', [f'ws://127.0.0.1:{remote_port}/proxy', requested])['result'] == {'success': 1}
          remote = remote_connected.get(timeout=5)
          token = remote.request.headers['Cookie'].removeprefix('jwt=')
          assert jwt.decode(token, env.public, algorithms=['ES256'])['identity'] == 'synthetic146'
          text = '한글 proxy\x00'
          payload = bytes(range(256)) * 4096
          remote.send(text)
          remote.send(payload)
          expected = text.encode() + payload
          actual = bytearray()
          frames = []
          while len(actual) < len(expected):
            chunk = remote.recv(timeout=12)
            assert isinstance(chunk, bytes)
            frames.append(len(chunk))
            actual.extend(chunk)
          assert actual == expected
          records.append({'requested_port': requested, 'bytes': len(actual), 'sha256': hashlib.sha256(actual).hexdigest(), 'binary_frames': len(frames), 'largest_frame': max(frames)})
          if requested == 22:
            remote.close()
        os.kill(pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        try:
          remote.recv(timeout=3)
          raise AssertionError('proxy remained open after global termination')
        except ConnectionClosed:
          pass
        result['returncode'] = process.returncode
        result['transfers'] = records
      trace = (args.output / 'sockets.log').read_text()
      assert 'IP_TOS, [144]' in trace
      result['ssh_tos'] = 144
      result['pass'] = True
    finally:
      result['owned_tcp_targets'] = targets
      (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: native RPC proxy whitelist,8022 migration,synthetic JWT,TOS144,fragmented TCP/binary WS bytes and global shutdown in an owned network namespace')


if __name__ == '__main__':
  main()
