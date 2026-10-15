"""Temporary unchanged-source provisioning pure/IO policy oracle; inert UART."""
import argparse
import ast
import hashlib
import json
from pathlib import Path


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  item = manifest['files']['tools/esp32s3_display/provision.py']
  data = item['content'].encode()
  assert hashlib.sha256(data).hexdigest() == item['sha256']
  tree = ast.parse(data)
  functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in ['validate_wifi', 'uart_request']]
  calls = []
  class Clock:
    value = 0
    @staticmethod
    def monotonic():
      result = Clock.value / 8
      Clock.value += 1
      calls.append(['clock', result])
      return result
  scope = dict(time=Clock, json=json)
  exec(compile(ast.Module(body=functions, type_ignores=[]), 'e8e152db:provision.py', 'exec'), scope)
  inputs, expected = [], []
  for ssid in ['', 'a', 's' * 32, 's' * 33, '가' * 10, '가' * 11, 'bad\0name']:
    for password in ['p' * 7, 'p' * 8, 'p' * 63, 'p' * 64, '가' * 3, 'pass\0word']:
      inputs.append(dict(kind='validate', ssid=ssid, password=password))
      try: scope['validate_wifi'](ssid, password); expected.append(dict(valid=True))
      except ValueError as error: expected.append(dict(error=str(error)))
  class Port:
    def __init__(self, lines): self.lines = iter(lines)
    def write(self, data): calls.append(['write', data.hex()])
    def readline(self):
      calls.append(['readline'])
      return next(self.lines, b'')
  for lines, timeout in [([b'boot\n', b'S3JSON bad\n', b'S3JSON {"saved":true}\n'], 2), ([b'S3JSON {"error":"synthetic"}\n'], 2), ([], 0), ([], .5), ([b'S3JSON {"error":false,"device":"001122334455"}\n'], 2)]:
    calls.clear(); Clock.value = 0
    inputs.append(dict(kind='uart', lines=[line.hex() for line in lines], timeout=timeout))
    try: result = dict(reply=scope['uart_request'](Port(lines), 'S3HELLO', timeout=timeout))
    except (ValueError, RuntimeError, TimeoutError) as error: result = dict(error=str(error))
    result['calls'] = list(calls); expected.append(result)
  fixture = dict(source_commit=manifest['source_commit'], input=inputs, expected=expected)
  (args.output / 'provision.json').write_text(json.dumps(fixture, indent=2) + '\n')
  print(json.dumps(dict(cases=len(inputs), scope='original credential validation/UART request; synthetic credentials and inert UART')))


if __name__ == '__main__': main()
