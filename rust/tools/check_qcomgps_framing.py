#!/usr/bin/env python3
"""Hosted-only original ModemDiag HDLC / native streaming comparison, issue #320."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import struct
import subprocess
from types import SimpleNamespace

from qcomgps_peer import crc


def original():
  path = Path('openpilot/system/qcomgpsd/modemdiag.py')
  source = path.read_bytes()
  tree = ast.parse(source)
  modem = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ModemDiag')
  methods = ast.Module(body=[node for node in modem.body if isinstance(node, ast.FunctionDef)
                            and node.name in ['hdlc_encapsulate', 'hdlc_decapsulate']], type_ignores=[])
  scope = {'pack': struct.pack}
  diag = SimpleNamespace(ESCAPE_CHAR=b'\x7d', TRAILER_CHAR=b'\x7e', ccitt_crc16=crc)
  scope['ModemDiag'] = diag
  exec(compile(methods, str(path), 'exec'), scope)
  return (lambda data: scope['hdlc_encapsulate'](diag, data),
          lambda data: scope['hdlc_decapsulate'](diag, data), hashlib.sha256(source).hexdigest())


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  encode, decode, source_hash = original()
  cases = []
  for opcode in range(256):
    message = bytes([opcode, 0x7d, 0x7e, 0, opcode])
    cases.append((f'opcode-{opcode}', encode(message), 1))
  for length in [1, 2, 3, 75, 65538, 65539]:
    for value in [0, 0x7d, 0x7e]:
      cases.append((f'boundary-{length}-{value}', encode(bytes([16]) + bytes([value]) * (length - 1)), 127))
  for name, wire in [('short', b'\x7e'), ('short2', b'\0\x7e'),
                     ('empty', encode(b'')), ('crc', b'\x10\0\0\x7e'),
                     ('escape-short', b'\x7d\x5e\x7e')]:
    cases.append((name, wire + encode(b'\x73\x03'), 2))
  rows, expected = [], []
  for name, wire, chunk in cases:
    output = []
    # Original recv splits only on raw delimiters, including corrupt frames.
    for raw in wire.split(b'\x7e')[:-1]:
      try:
        payload = decode(raw + b'\x7e')
        output.append({'opcode': payload[0], 'payload': list(payload[1:])})
      except (AssertionError, IndexError):
        output.append({'error': True})
    expected.append(output)
    rows.append({'chunks': [list(wire[index:index + chunk]) for index in range(0, len(wire), chunk)]})
  completed = subprocess.run([args.binary], input=''.join(json.dumps(row) + '\n' for row in rows),
                             text=True, capture_output=True, timeout=60, check=True)
  actual = [json.loads(line) for line in completed.stdout.splitlines()]
  assert len(actual) == len(cases)
  results = []
  for (name, _, _), want, got in zip(cases, expected, actual, strict=True):
    assert len(got) == len(want), name
    for wanted, observed in zip(want, got, strict=True):
      if 'error' in wanted:
        assert 'error' in observed, (name, wanted, observed)
      else:
        assert observed == wanted, name
    results.append({'case': name, 'frames': len(got), 'pass': True})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps({'source_sha256': source_hash, 'cases': results}, indent=2) + '\n')
  print(f'PASS: {len(cases)} unchanged-source/native streaming HDLC comparisons')


if __name__ == '__main__':
  main()
