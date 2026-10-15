"""Temporary pinned-source UDP discovery oracle; inert sockets only."""
import argparse
import json
from pathlib import Path
from check_cluster_wifi_protocol import source_module


def capture(module, fault, timeout):
  config = dict(version=1, device='001122334455', key='ab' * 32)
  nonce = bytes(range(16))
  prefix = module.FOUND + bytes.fromhex(config['device']) + nonce
  valid = prefix + module.mac(bytes.fromhex(config['key']), prefix)
  replies = []
  if fault == 'malformed':
    replies = [valid[:57], b'bad!' + valid[4:], valid[:10] + b'x' + valid[11:], valid[:-1] + bytes([valid[-1] ^ 1])]
  replies.append(valid)
  calls = []
  ticks = iter(index / 8 for index in range(100))
  class Datagram:
    def __enter__(self): return self
    def __exit__(self, *_): calls.append(['close'])
    def setsockopt(self, *_): calls.append(['broadcast'])
    def sendto(self, request, address):
      calls.append(['send', request.hex(), address[0], address[1]])
      if fault == 'send_error': raise OSError('send failed')
    def settimeout(self, seconds): calls.append(['timeout', seconds])
    def recvfrom(self, maximum):
      calls.append(['receive', maximum])
      if fault == 'timeout': raise TimeoutError('timed out')
      if fault == 'receive_error': raise OSError('receive failed')
      return replies.pop(0), ('192.0.2.8', 3333)
  def clock():
    value = next(ticks)
    calls.append(['clock', value])
    return value
  def token(size):
    calls.append(['nonce', size])
    if fault == 'nonce_error': raise OSError('nonce failed')
    return nonce
  def socket(*_):
    calls.append(['socket'])
    if fault == 'socket_error': raise OSError('socket failed')
    return Datagram()
  old = module.time.monotonic, module.secrets.token_bytes, module.socket.socket
  module.time.monotonic, module.secrets.token_bytes, module.socket.socket = clock, token, socket
  try:
    try: result = dict(address=module.discover(config, timeout=timeout, targets=('127.0.0.1', '192.0.2.1')))
    except OSError as error: result = dict(error=str(error))
  finally:
    module.time.monotonic, module.secrets.token_bytes, module.socket.socket = old
  result['calls'] = calls
  return dict(config=config, fault=fault, timeout=timeout, replies=[value.hex() for value in ([valid[:57], b'bad!' + valid[4:], valid[:10] + b'x' + valid[11:], valid[:-1] + bytes([valid[-1] ^ 1])] if fault == 'malformed' else []) + [valid]]), result


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  module = source_module()
  cases = [capture(module, fault, timeout) for fault, timeout in [('valid', 2.), ('malformed', 2.), ('timeout', 2.), ('valid', 0.), ('valid', -1.), ('valid', .125), ('send_error', 2.), ('receive_error', 2.), ('nonce_error', 2.), ('socket_error', 2.)]]
  fixture = dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', input=[case for case, _ in cases], expected=[result for _, result in cases])
  (args.output / 'discovery.json').write_text(json.dumps(fixture, indent=2) + '\n')
  print(json.dumps(dict(cases=len(cases), source_commit=fixture['source_commit'], scope='inert UDP discovery; no device or network IO')))


if __name__ == '__main__': main()
