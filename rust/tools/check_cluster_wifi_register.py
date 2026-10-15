"""Temporary original UART registration sequence oracle with inert serial module."""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import sys
from types import ModuleType


def main():
  parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  item = manifest['files']['tools/esp32s3_display/provision.py']
  source = item['content']
  data = source.encode()
  assert hashlib.sha256(data).hexdigest() == item['sha256']
  assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == item['git_blob']
  tree = ast.parse(source)
  functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ['validate_wifi', 'uart_request', 'register']]
  inputs, expected = [], []
  for fault in ['none', 'unicode', 'delete', 'version', 'version_true', 'device_short', 'saved_false', 'saved_one', 'changed', 'rejected', 'timeout']:
    calls = []; tick = [0]
    ssid = '차량😀' if fault == 'unicode' else 'car\x7f' if fault == 'delete' else 'synthetic-network'
    password = 'synthetic-password'
    hello = dict(version=True if fault == 'version_true' else 2 if fault == 'version' else 1, device='001122' if fault == 'device_short' else '001122334455')
    config = dict(saved=1 if fault == 'saved_one' else fault != 'saved_false')
    pairing = dict(version=1, device='001122334456' if fault == 'changed' else '001122334455', key='ab' * 32)
    replies = [hello, config, pairing]
    if fault == 'rejected': replies[0] = dict(error='synthetic')
    lines = [] if fault == 'timeout' else [('S3JSON ' + json.dumps(reply) + '\n').encode() for reply in replies]
    remaining = iter(lines)
    class Clock:
      @staticmethod
      def monotonic():
        value = tick[0] / 8; tick[0] += 1; calls.append(['clock', value]); return value
    class Port:
      def open(self): calls.append(['open', self.port, self.baudrate, self.timeout, self.dtr, self.rts])
      def reset_input_buffer(self): calls.append(['reset'])
      def close(self): calls.append(['close'])
      def write(self, data): calls.append(['write', data.hex()])
      def readline(self): calls.append(['readline']); return next(remaining, b'')
    serial = ModuleType('serial'); serial.Serial = Port
    old = sys.modules.get('serial'); sys.modules['serial'] = serial
    scope = dict(time=Clock, json=json)
    exec(compile(ast.Module(body=functions, type_ignores=[]), 'e8e152db:provision.py', 'exec'), scope)
    try:
      try: result = dict(pairing=scope['register']('/inert/uart', ssid, password))
      except (ValueError, RuntimeError, TimeoutError) as error: result = dict(error=str(error))
    finally:
      if old is None: sys.modules.pop('serial', None)
      else: sys.modules['serial'] = old
    inputs.append(dict(fault=fault, ssid=ssid, password=password, lines=[line.hex() for line in lines]))
    result['calls'] = calls; expected.append(result)
  (args.output / 'register.json').write_text(json.dumps(dict(source_commit=manifest['source_commit'], input=inputs, expected=expected), indent=2) + '\n')
  print(json.dumps(dict(cases=len(inputs), scope='original complete register sequence; inert UART/synthetic credentials')))


if __name__ == '__main__': main()
