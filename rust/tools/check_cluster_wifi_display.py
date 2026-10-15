"""Temporary original display queue oracle with inert base/connection objects."""
import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import sys
from types import ModuleType


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  item = manifest['files']['openpilot/selfdrive/carrot/cluster/cluster_wifi_display.py']
  source = item['content'].encode()
  assert hashlib.sha256(source).hexdigest() == item['sha256']
  assert hashlib.sha1(b'blob ' + str(len(source)).encode() + b'\0' + source).hexdigest() == item['git_blob']
  usb, protocol = ModuleType('cluster_usb_display'), ModuleType('cluster_wifi_protocol')
  calls = []
  class Lock:
    def __enter__(self): calls.append(['lock'])
    def __exit__(self, *_): calls.append(['unlock'])
  class Base:
    def __init__(self, **_): self._usb_lock = Lock(); self._disconnected = False
  usb.TuringUsbDisplay = Base
  usb._set_cluster_hud_connected = lambda value: calls.append(['connected', value])
  protocol.Connection = object
  protocol.load_config = lambda: None
  old = {name: sys.modules.get(name) for name in [usb.__name__, protocol.__name__]}
  sys.modules.update({usb.__name__: usb, protocol.__name__: protocol})
  module = ModuleType('source_wifi_display')
  try: exec(compile(source, 'e8e152db:cluster_wifi_display.py', 'exec'), module.__dict__)
  finally:
    for name, value in old.items():
      if value is None: sys.modules.pop(name, None)
      else: sys.modules[name] = value
  class Connection:
    def __init__(self): self.seq = 0; self.responses = deque(); self.fault = None
    def send(self, payload, *, expect_reply):
      calls.append(['send', payload.hex(), expect_reply])
      if self.fault == 'send': raise OSError('send failed')
      self.seq += 1
      self.responses.append((self.seq + (self.fault == 'sequence'), bytes([self.seq]), 123))
      return self.seq
    def result(self):
      calls.append(['result'])
      if self.fault == 'result': raise OSError('result failed')
      return self.responses.popleft()
    def close(self): calls.append(['close'])
  scenarios = [
    [('frame', None)] * 7 + [('command', None), ('no_ack', None)],
    [('frame', None)] * 3 + [('command', None)],
    [('command', 'sequence')], [('command', 'send')], [('command', 'result')],
    [('frame', None)] * 3 + [('frame', 'result')],
    [('closed', None)],
  ]
  inputs, expected = [], []
  for scenario in scenarios:
    calls.clear()
    display = module.S3WifiDisplay({})
    display.connection = Connection()
    operations, results = [], []
    for index, (kind, fault) in enumerate(scenario):
      payload = bytes([index + 1]) * 512
      operations.append(dict(kind=kind, fault=fault, payload=payload.hex()))
      if kind == 'closed': display.connection = None
      if display.connection is not None: display.connection.fault = fault
      try:
        response = display._transaction(payload, frame=kind == 'frame', expect_reply=kind != 'no_ack')
        result = dict(response=response.hex())
      except RuntimeError as error:
        result = dict(error=str(error), cause=str(error.__cause__))
      result.update(pending=list(display.pending), disconnected=display._disconnected, connected=display.connection is not None, calls=list(calls))
      results.append(result)
      calls.clear()
    inputs.append(operations); expected.append(results)
  fixture = dict(source_commit=manifest['source_commit'], input=inputs, expected=expected)
  (args.output / 'display.json').write_text(json.dumps(fixture, indent=2) + '\n')
  print(json.dumps(dict(scenarios=len(inputs), transactions=sum(map(len, inputs)), scope='original display FIFO/backpressure/errors; inert IO')))


if __name__ == '__main__': main()
