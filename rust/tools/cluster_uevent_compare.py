#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python3 rust/tools/cluster_uevent_compare.py ROOT NATIVE OUTPUT
"""Compare original socket routines and Rust against actual owned datagrams."""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time


def source(root: Path, case_path: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster')]
  path = root / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  tree = ast.parse(path.read_text())
  names = {'_open_usb_uevent_socket', '_decode_uevent', '_parse_hex_int', '_usb_uevent_matches', '_wait_for_usb_uevent'}
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
  assert {node.name for node in nodes} == names
  namespace = {'socket': socket, 'os': os, 'select': select, 'time': time, 'NETLINK_KOBJECT_UEVENT': 15}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  case = json.loads(case_path.read_text())
  if case['op'] == 'factory':
    receiver = namespace['_open_usb_uevent_socket']()
    result = receiver is not None
    if receiver is not None:
      receiver.close()
  else:
    receiver, sender = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    receiver.setblocking(False)
    try:
      for packet in case['packets']:
        sender.send(bytes(packet))
      result = [namespace['_wait_for_usb_uevent'](None if case['absent'] else receiver, timeout / 1000, case['expected']) for timeout in case['timeouts_ms']]
    finally:
      receiver.close()
      sender.close()
  print(json.dumps(result))


def compare(root: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  packet = b'add@/devices/owned\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0'
  other = b'remove@/devices/owned\0SUBSYSTEM=usb\0PRODUCT=1cbe/92/1\0'
  rows = []
  cases = [('kernel-factory', {'op': 'factory'})]
  for name, packets, timeouts, absent in [
    ('matching-then-unmatched', [packet, other], [50, 1], False),
    ('unmatched-then-matching', [other, packet], [50, 1], False),
    ('zero-timeout', [packet], [0, 50], False),
    ('empty', [], [1], False),
    ('absent', [], [1], True),
    ('truncated', [b'add@/owned\0SUBSYSTEM=usb\0' + b'X' * 8200 + b'\0PRODUCT=1cbe/92/1\0'], [50, 1], False),
  ]:
    cases.append((name, {'op': 'wait', 'packets': [list(payload) for payload in packets], 'timeouts_ms': timeouts, 'absent': absent, 'expected': 0x92}))
  for name, case in cases:
    destination = output / name
    destination.mkdir()
    case_path = destination / 'input.json'
    case_path.write_text(json.dumps(case) + '\n')
    results = []
    for implementation in ['source', 'native']:
      argv = [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(case_path)] if implementation == 'source' else [str(native)]
      completed = subprocess.run(argv, input=None if implementation == 'source' else json.dumps(case), capture_output=True, text=True, timeout=10, check=False)
      receipt = {'argv': argv, 'returncode': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}
      (destination / (implementation + '-invocation.json')).write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      results.append(json.loads(completed.stdout.splitlines()[-1]))
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'pass': results[0] == results[1]}
    if name == 'kernel-factory':
      assert results == [True, True], row
    assert row['pass'], row
    (destination / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    rows.append(row)
  result = {
    'pass': True,
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': 'actual kernel netlink construction and owned datagram drain/timeout/truncation; no physical USB hotplug or full autorun proof',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  match sys.argv[1:]:
    case ['--source', root, case_path]:
      source(*(Path(value).resolve() for value in [root, case_path]))
    case [root, native, output]:
      compare(*(Path(value).resolve() for value in [root, native, output]))
    case _:
      raise SystemExit('ROOT NATIVE OUTPUT | --source ROOT CASE')
