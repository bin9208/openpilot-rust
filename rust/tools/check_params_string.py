#!/usr/bin/env python3
"""Compare physical Params reads and real PUSH logging with the original Cython getter."""

import argparse
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile

import zmq
from original_params_binding import load


def receive(socket, marker):
  records = []
  while True:
    packet = socket.recv()
    record = json.loads(packet[1:])
    assert packet[0] == record['levelnum'], record
    if record['msg'] == marker:
      return records
    assert record['levelnum'] == 30 and record['level'] == 'WARNING', record
    records.append({'packet_hex': packet.hex(), 'record': record})


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--native', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='params-string-') as temporary:
    root = Path(temporary)
    os.environ['OPENPILOT_PREFIX'] = 'fixture'
    context = zmq.Context()
    source_socket, native_socket = [context.socket(zmq.PULL) for _ in range(2)]
    source_endpoint, native_endpoint = ['ipc://' + str(root / name) for name in ['source-log', 'native-log']]
    for socket, endpoint in [(source_socket, source_endpoint), (native_socket, native_endpoint)]:
      socket.setsockopt(zmq.RCVTIMEO, 5000)
      socket.bind(endpoint)
    module, swaglog = load(args.binding.resolve(), source_endpoint, root / 'logs')
    params = module.Params(str(root / 'source'))
    source_keys = sorted(key.decode() for key in params.all_keys() if params.get_type(key) == module.ParamKeyType.STRING)
    with (args.output / 'native-stderr.log').open('w') as stderr:
      process = subprocess.Popen(
        [str(args.native.resolve()), str(root / 'native'), 'fixture', native_endpoint], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True
      )
      try:
        native_keys = json.loads(process.stdout.readline())['keys']
        assert native_keys == source_keys, (native_keys, source_keys)
        cases = [(f'absent-{key}', key, None, None) for key in source_keys]
        cases += [('empty', 'DongleId', b'', None), ('ascii', 'DongleId', b'normal', None), ('unicode-nul', 'DongleId', '한글\0😀\ufeff'.encode(), None)]
        cases += [(f'repr-{byte}', 'DongleId', b'\xff' + bytes([byte]), None) for byte in range(256)]
        cases += [
          (f'utf8-{index}', 'AthenadRecentlyViewedRoutes', data, None)
          for index, data in enumerate([b'\xc0\x80', b'\xed\xa0\x80', b'\xf4\x90\x80\x80', b'\xe2\x82', b'\xef\xbb\xbf', b'\xff\'"\n', b"one'\xff"])
        ]
        rng = random.Random(64)
        cases += [(f'random-{index}', 'DongleId', rng.randbytes(rng.randrange(1, 80)), None) for index in range(500)]
        cases += [
          ('permission', 'DongleId', b'\xff', 'permission'),
          ('directory', 'DongleId', None, 'directory'),
          ('unknown', 'NotARegisteredKey', b'\xff', None),
        ]
        results = []
        for index, (name, key, data, special) in enumerate(cases):
          paths = [root / kind / 'fixture' / key for kind in ['source', 'native']]
          for path in paths:
            if path.exists():
              path.rmdir() if path.is_dir() else path.unlink()
            if data is not None:
              path.write_bytes(data)
            if special == 'permission':
              path.chmod(0)
            if special == 'directory':
              path.mkdir()
          try:
            expected = {'value': params.get(key)}
          except module.UnknownKeyName:
            expected = {'error': 'UnknownKey'}
          marker = f'barrier-{index}'
          swaglog.cloudlog.debug(marker)
          original_records = receive(source_socket, marker)
          process.stdin.write(json.dumps({'key': key, 'marker': marker}) + '\n')
          process.stdin.flush()
          actual = json.loads(process.stdout.readline())
          native_records = receive(native_socket, marker)
          assert actual == expected, (name, actual, expected)
          assert [row['record']['msg'] for row in original_records] == [row['record']['msg'] for row in native_records], name
          for row in native_records:
            record = row['record']
            assert record['filename'] == 'lib.rs' and record['funcName'] == 'openpilot_params_typed::get_string'
            assert record['lineno'] > 0 and 'params-typed/src/lib.rs' in record['pathname'], record
          results.append(
            {
              'name': name,
              'key': key,
              'input_hex': None if data is None else data.hex(),
              'source': expected,
              'native': actual,
              'source_records': original_records,
              'native_records': native_records,
            }
          )
          for path in paths:
            if special == 'permission':
              path.chmod(0o600)
        for kind in ['source', 'native']:
          (root / kind / 'fixture' / 'AthenadRecentlyViewedRoutes').write_bytes(b'\xff')
        swaglog.ipchandler.sock.close()
        try:
          params.get('AthenadRecentlyViewedRoutes')
          raise AssertionError('closed source logger accepted a warning')
        except zmq.ZMQError as error:
          assert error.errno == zmq.ENOTSOCK
        process.stdin.write(json.dumps({'key': 'AthenadRecentlyViewedRoutes', 'marker': '', 'close': True}) + '\n')
        process.stdin.flush()
        assert json.loads(process.stdout.readline()) == {'error': 'Logging'}
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        (args.output / 'comparison.json').write_text(
          json.dumps({'passed': True, 'string_keys': source_keys, 'cases': results, 'closed_logger': 'both ENOTSOCK/error; no fallback value'}, indent=2) + '\n'
        )
        print(json.dumps({'passed': True, 'cases': len(results), 'string_keys': len(source_keys), 'closed_logger': True}))
      finally:
        if process.poll() is None:
          process.kill()
          process.wait()
        swaglog.ipchandler.close()
        source_socket.close()
        native_socket.close()
        context.term()


if __name__ == '__main__':
  main()
