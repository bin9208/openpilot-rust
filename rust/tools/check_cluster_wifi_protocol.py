"""Temporary final-source S3 configuration/framing fixture; never runtime.

Uses isolated files/inert sockets only. A native protocol trace can replay the
same fixture; source firmware or S3 interoperability is a separate gate.
"""
import argparse
import hashlib
import hmac
import json
from pathlib import Path
import struct
import tempfile
import stat
from types import ModuleType


def source_module():
  fixture = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  assert fixture['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
  item = fixture['files']['openpilot/selfdrive/carrot/cluster/cluster_wifi_protocol.py']
  data = item['content'].encode()
  assert hashlib.sha256(data).hexdigest() == item['sha256']
  assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == item['git_blob']
  module = ModuleType('pinned_cluster_wifi_protocol')
  exec(compile(data, 'e8e152db:cluster_wifi_protocol.py', 'exec'), module.__dict__)
  return module


class Socket:
  def __init__(self, response=b'', fragment=7):
    self.data, self.fragment, self.offset = response, fragment, 0
    self.reads, self.writes = [], []
    self.calls = []
  def recv(self, count):
    self.reads.append(count)
    size = min(count, self.fragment, len(self.data) - self.offset)
    result = self.data[self.offset:self.offset + size]
    self.offset += size
    return result
  def sendall(self, data):
    self.writes.append(data)
  def setsockopt(self, level, option, value):
    self.calls.append(['option', level, option, value])
  def shutdown(self, direction):
    self.calls.append(['shutdown', direction])
  def close(self):
    self.calls.append(['close'])


def payload(size, seed):
  return bytes((index * 17 + seed) % 256 for index in range(size))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  module = source_module()
  cases, expected = [], []
  base = dict(version=1, device='001122334455', key='ab' * 32)
  variants = [base, {}, [], None, dict(base, password='synthetic-not-a-credential', extra=[1, 2])]
  for key, values in [
    ('version', [None, False, True, 0, 1., 2, '1']),
    ('device', ['', '00 11\t22\n33\r44\v55', '0 01122334455', '00112233445', '0011223344gg', '00\u20031122334455', 3]),
    ('key', ['', 'ab ' * 32, 'ab' * 31, 'ab' * 33, 'ab\t' * 31 + 'ab', None]),
    ('fps', [False, True, 0, 1, 20, 21, 15., '15', None]),
    ('quality', [False, 19, 20, 90, 91, 50., '50', None]),
  ]:
    variants.extend(dict(base, **{key: value}) for value in values)
  with tempfile.TemporaryDirectory(prefix='s3-config-reference-') as directory:
    path = Path(directory) / 'registration.json'
    cases.append(dict(kind='config_missing'))
    expected.append(dict(config=module.load_config(path)))
    for value in variants:
      raw = json.dumps(value)
      path.write_text(raw)
      cases.append(dict(kind='config', raw=raw))
      try:
        config = module.load_config(path)
        expected.append(dict(config=config, device=list(bytes.fromhex(config['device'])), key=list(bytes.fromhex(config['key'])), fps=config.get('fps', 15), quality=config.get('quality', 50)))
      except RuntimeError as error:
        expected.append(dict(error=str(error)))
    for value in [base, dict(base, version=True), dict(base, version=1.),
                  dict(base, fps=20, quality=90, password='synthetic-not-a-credential'),
                  dict(base, fps=0), dict(base, quality=91), dict(base, device='bad'),
                  dict(base, key='00'), dict(device=base['device'], key=base['key'])]:
      path.write_text('unchanged')
      path.chmod(0o644)
      cases.append(dict(kind='config_save', config=value))
      try:
        module.save_config(value, path)
        result = dict(saved=True)
      except (RuntimeError, KeyError) as error:
        result = dict(error=str(error))
      result.update(file=path.read_text(), mode=stat.S_IMODE(path.stat().st_mode), temporary_files=len(list(path.parent.glob(path.name + '.*.tmp'))))
      expected.append(result)
  for sequence in [0, 1, 0xfffffffe, 0xffffffff]:
    for size in [0, 511, 512, 513, 4096, 1048576, 1048577]:
      for reply in [False, True]:
        seed, key = 37, bytes(range(32))
        connection = module.Connection.__new__(module.Connection)
        connection.sock, connection.sequence, connection.session_key = Socket(), sequence, key
        cases.append(dict(kind='send', sequence=sequence, size=size, seed=seed, reply=reply, key=list(key)))
        try:
          result = connection.send(payload(size, seed), expect_reply=reply)
          wire = b''.join(connection.sock.writes)
          expected.append(dict(sequence=result, length=len(wire), sha256=hashlib.sha256(wire).hexdigest(), prefix=wire[:56].hex()))
        except (ValueError, struct.error) as error:
          expected.append(dict(error=str(error), sequence=connection.sequence, writes=len(connection.sock.writes)))
  key = bytes(range(32))
  for size in [0, 1, 512, 513]:
    for fault in ['none', 'magic', 'reserved', 'status', 'signature', 'status_signature', 'short_header', 'short_signature', 'short_payload']:
      response = payload(size, 91)
      header = module.HEADER.pack(module.ACK + int(fault == 'magic'), 19, size, int(fault in ['status', 'status_signature']), 12345, int(fault == 'reserved'))
      signature = module.mac(key, header + response)
      if fault in ['signature', 'status_signature']:
        signature = bytes([signature[0] ^ 1]) + signature[1:]
      wire = header + signature + response
      if fault == 'short_header': wire = wire[:23]
      if fault == 'short_signature': wire = wire[:55]
      if fault == 'short_payload' and size: wire = wire[:-1]
      connection = module.Connection.__new__(module.Connection)
      connection.sock, connection.session_key = Socket(wire), key
      cases.append(dict(kind='ack', wire=wire.hex(), key=list(key), fragment=7))
      try:
        sequence, data, usb_us = connection.result()
        result = dict(sequence=sequence, response=data.hex(), usb_us=usb_us)
      except ConnectionError as error:
        result = dict(error=str(error))
      result.update(consumed=connection.sock.offset, reads=connection.sock.reads)
      expected.append(result)
  for fault in ['none', 'identity', 'server_signature', 'short_hello', 'short_server']:
    device = bytes.fromhex(base['device'])
    key = bytes.fromhex(base['key'])
    nonce = bytes(range(32))
    hello = module.HELLO + device + bytes(range(32, 64))
    if fault == 'identity': hello = b'bad!' + hello[4:]
    reply = module.mac(key, b'server' + hello + nonce)
    if fault == 'server_signature': reply = bytes([reply[0] ^ 1]) + reply[1:]
    wire = hello + reply
    if fault == 'short_hello': wire = wire[:41]
    if fault == 'short_server': wire = wire[:73]
    sock = Socket(wire)
    original_connect, original_nonce = module.socket.create_connection, module.secrets.token_bytes
    nonce_calls = []
    module.socket.create_connection = lambda *_args, **_kwargs: sock
    module.secrets.token_bytes = lambda size: nonce_calls.append(size) or nonce
    cases.append(dict(kind='session', config=base, wire=wire.hex(), nonce=list(nonce), fragment=7))
    try:
      try:
        connection = module.Connection(base, host='127.0.0.1')
        result = dict(session_key=connection.session_key.hex(), sequence=connection.sequence)
      except ConnectionError as error:
        result = dict(error=str(error))
    finally:
      module.socket.create_connection, module.secrets.token_bytes = original_connect, original_nonce
    result.update(consumed=sock.offset, reads=sock.reads, writes=[value.hex() for value in sock.writes], calls=sock.calls, nonce_calls=nonce_calls)
    expected.append(result)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  receipt = dict(cases=len(cases), source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', scope='source configuration/frame/ACK behavior; isolated files and inert sockets', native_compared=False)
  (args.output / 'receipt.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
