#!/usr/bin/env python3
import argparse
import ast
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from types import SimpleNamespace
from athena_fixture import private_environment, websocket_server, wait_for
from athena_reference import ROOT


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  result = {'pass': False}
  with websocket_server() as (port, connected, _), private_environment(port) as env:
    root = env.home / 'log'
    now = int(time.time())
    cache = {}
    def attr(path, name):
      if path not in cache:
        try:
          cache[path] = os.getxattr(path, name)
        except OSError:
          cache[path] = None
      return cache[path]
    scope = {'time': SimpleNamespace(time=lambda: now), 'os': os, 'sys': sys, 'Paths': SimpleNamespace(swaglog_root=lambda: str(root)), 'getxattr': attr, 'LOG_ATTR_NAME': 'user.upload'}
    tree = ast.parse((ROOT / 'openpilot/system/athena/athenad.py').read_text())
    body = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'get_logs_to_send_sorted']
    exec(compile(ast.Module(body=body, type_ignores=[]), 'athenad.py', 'exec'), scope)
    for name, stamp in [('a', None), ('b', now - 3601), ('c', now - 3600), ('d', 2147483647), ('z', None)]:
      (root / name).write_text(name + '\r\ntext\r')
      if stamp is not None:
        os.setxattr(root / name, 'user.upload', stamp.to_bytes(4, sys.byteorder))
    scan = subprocess.Popen([args.binary, 'scan'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env.env)
    observations = []
    try:
      for mutation in [False, True]:
        if mutation:
          os.setxattr(root / 'a', 'user.upload', (2147483647).to_bytes(4, sys.byteorder))
        scan.stdin.write(json.dumps({'root': str(root), 'now': now}) + '\n')
        scan.stdin.flush()
        actual = json.loads(scan.stdout.readline())
        expected = scope['get_logs_to_send_sorted']()
        assert actual == expected == ['a', 'b'], (actual, expected)
        observations.append(actual)
    finally:
      scan.stdin.close()
      assert scan.wait(timeout=5) == 0
    result['source_scan_and_cached_external_mutation'] = observations
    os.removexattr(root / 'a', 'user.upload')
    stats = env.home / 'stats'
    (stats / 'tmp-current').write_text('must remain')
    (stats / 'first').write_bytes(b'first\r\nline\rlast')
    with (args.output / 'daemon.log').open('wb') as output:
      process = subprocess.Popen([args.binary, 'session'], env=env.env, stdout=output, stderr=subprocess.STDOUT)
      try:
        peer = connected.get(timeout=10)
        wait_for(lambda: len([row for row in peer.messages if row['packet'].get('method') == 'forwardLogs']) == 2, 8)
        wait_for(lambda: any(row['packet'].get('method') == 'storeStats' for row in peer.messages), 5)
        for name in ['a', 'b']:
          wait_for(lambda: int.from_bytes(os.getxattr(root / name, 'user.upload'), sys.byteorder) == 2147483647)
        assert (stats / 'tmp-current').exists() and not (stats / 'first').exists()
        logs = [row['packet'] for row in peer.messages if row['packet'].get('method') == 'forwardLogs']
        assert [packet['id'] for packet in logs] == ['b', 'a']
        assert all(packet['params']['logs'] == packet['id'] + '\ntext\n' for packet in logs)
        stat = next(row['packet'] for row in peer.messages if row['packet'].get('method') == 'storeStats')
        assert stat['params']['stats'] == 'first\nline\nlast'
        result.update(messages=peer.messages, successful_ack_xattrs={name: os.getxattr(root / name, 'user.upload').hex() for name in ['a', 'b']}, temporary_stats_preserved=True)
        os.kill(process.pid, signal.SIGTERM)
        assert process.wait(timeout=8) == 0
        result.update(returncode=process.returncode, **{'pass': True})
      finally:
        if process.poll() is None:
          process.send_signal(signal.SIGTERM)
          process.wait(timeout=10)
        (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print('PASS: unchanged-source log scan,one-hour boundary,persistent xattr cache,newest eligible first,real websocket log/stat forwarding,ACK timestamps,newline conversion,temporary stats and cleanup')


if __name__ == '__main__':
  main()
