import argparse
import gzip
import json
from pathlib import Path
import urllib.request

import brotli

from check_crash_sdk_transport import Receiver, decode_envelope


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  event = {'message': 'local encoding fixture', 'tags': {'fixture': '한글'}}
  payload = json.dumps(event).encode()
  body = b'{}\n' + json.dumps({'type': 'event', 'length': len(payload)}).encode() + b'\n' + payload
  receiver = Receiver()
  rows = []
  try:
    url = f'http://127.0.0.1:{receiver.server.server_port}/api/1/envelope/'
    for encoding, wire in [(None, body), ('gzip', gzip.compress(body)), ('br', brotli.compress(body))]:
      headers = {'Content-Type': 'application/x-sentry-envelope'}
      if encoding is not None:
        headers['Content-Encoding'] = encoding
      request = urllib.request.Request(url, data=wire, headers=headers)
      with urllib.request.urlopen(request, timeout=5) as response:
        assert response.status == 200 and response.read() == b'{}'
      captured = receiver.events.get(timeout=5)
      assert captured['event'] == event and captured['raw'].encode() == body
      assert captured['wire_hex'] == wire.hex() and captured['content_encoding'] == encoding
      rows.append(captured)
    assert receiver.events.empty()
    try:
      decode_envelope(body, 'unsupported-fixture')
    except ValueError:
      pass
    else:
      raise AssertionError('unsupported encoding was accepted')
  finally:
    receiver.close()
  args.output.write_text(json.dumps({'result': 'PASS', 'http_cases': rows, 'unsupported_rejected': True}, indent=2) + '\n')
  print('PASS: 3 actual HTTP envelope encodings; unsupported encoding rejected')


if __name__ == '__main__':
  main()
