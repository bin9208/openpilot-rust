# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0", "requests==2.34.2", "pyserial==3.5"]
# ///
"""Execute original journald and its Rust replacement with synthetic OS output."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import random
import signal
import struct
import subprocess

from journald_peer import Peer


def valid_records():
  records = [
    '{}',
    '{"MESSAGE":"plain", "__REALTIME_TIMESTAMP":"1723456789012345", "_PID":"123", "PRIORITY":"6", "SYSLOG_IDENTIFIER":"kernel"}',
    '{"z":"한😀\\n\\t\\u0000", "a":[0,255,{"b":true,"a":null}], "float":181449526438435.12, "negzero":-0.0}',
    '{"z":1,"a":2,"z":3,"_PID":-2147483648,"PRIORITY":255,"__REALTIME_TIMESTAMP":18446744073709551615}',
    '{"_PID":1.9,"PRIORITY":true,"__REALTIME_TIMESTAMP":12.8,"SYSLOG_IDENTIFIER":"한"}',
    '{"_PID":" +1_234 ","PRIORITY":"٠٧","__REALTIME_TIMESTAMP":"１２３"}',
    '{"large":1234567890123456789012345678901234567890,"tiny":1e-7,"huge":1e300}',
    '{"special":[NaN,Infinity,-Infinity,1e999],"surrogate":"\\ud800"}',
    '{"\\ud800":1,"slash":"\\/","pair":"\\ud800\\udc00","negative":-0,"SYSLOG_IDENTIFIER":""}',
    '{"nested":' + '[' * 5000 + '0' + ']' * 5000 + '}',
  ]
  rng = random.Random(65)
  for index in range(128):
    value = struct.unpack('d', rng.getrandbits(64).to_bytes(8, 'little'))[0]
    records.append(json.dumps({'first': index, 'number': value, 'wide': rng.getrandbits(384),
                              'nested': [None, True, False, '한😀\x00\x7f\udfff', {'z': 1, 'a': 2}]}))
  return records


def entry(text):
  value = json.loads(text)
  result = {'valid': False, 'androidLog': {'id': 0, 'ts': int(value.get('__REALTIME_TIMESTAMP', 0)),
    'priority': int(value.get('PRIORITY', 0)), 'pid': int(value.get('_PID', 0)), 'tid': 0,
    'tag': value.get('SYSLOG_IDENTIFIER', ''), 'message': json.dumps(value)}}
  if 'SYSLOG_IDENTIFIER' not in value:
    del result['androidLog']['tag']
  return result


def normal(binary, output):
  peer = Peer(output, binary)
  try:
    for text in valid_records():
      peer.send(('  ' + text + ' \n').encode())
      assert peer.receive('androidLog') == entry(text), text
    # Universal newline decoding and fragmented UTF-8 come from the real child pipe.
    peer.send(b'\r\n \t\r{}\r{}\r\n{"MESSAGE":"\xed\x95')
    assert peer.receive('androidLog') == entry('{}')
    assert peer.receive('androidLog') == entry('{}')
    peer.send(b'\x9c"}\n')
    assert peer.receive('androidLog') == entry('{"MESSAGE":"한"}')
    for payload in (b'{bad\n', b'{"a":]\n', b'{"x":1} trailing\n', b'{"x":01}\n',
                    b'{"x":+1}\n', b'{"x":.1}\n', b'{"x":1.}\n', b'{"x":true,}\n',
                    b'{"x":[1,]}\n', b'{"x":"\\q"}\n', b'{"x":"\\uZZZZ"}\n',
                    b'{"x":Inf}\n', b'{"x":"literal\x01"}\n'):
      peer.send(payload + b'{}\n')
      assert peer.receive('androidLog') == entry('{}')
      records = [json.loads(peer.receive(topic)[topic]) for topic in ('logMessage', 'errorLogMessage')]
      assert records[0] == records[1]
      record = records[0]
      assert record['msg'] == 'failed to parse journalctl output'
      assert record['level'] == 'ERROR' and record['levelnum'] == 40 and record['exc_info']
      assert record['process'] == peer.process.pid
      if binary is None:
        assert record['filename'] == 'journald.py' and record['funcName'] == 'main'
      else:
        assert record['filename'].endswith('.rs') and 'journald' in record['module']
        assert record['ctx']['daemon'] == 'journald' and record['ctx']['runtime_language'] == 'rust'
        assert record['ctx']['source_commit'] == subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        path = Path(record['pathname'])
        if not path.is_absolute():
          path = Path('rust') / path
        assert 'log_site!' in path.read_text().splitlines()[record['lineno'] - 1]
    peer.send(b'{"MESSAGE":"last without newline"}')
    peer.command({'op': 'close'})
    assert peer.receive('androidLog') == entry('{"MESSAGE":"last without newline"}')
    result = peer.finish()
    assert result['exit'] == 0 and result['trace'][-1]['event'] == 'signal-15'
    assert peer.subscribers['androidLog'].receive(non_blocking=True) is None
    disk = [json.loads(line) for path in peer.log_root.glob('swaglog.*') for line in path.read_text().splitlines()]
    assert len(disk) == len(peer.records['errorLogMessage'])
    assert all(row['levelnum'] == 40 and row['msg$s'] == 'failed to parse journalctl output' for row in disk)
    return {'messages': len(peer.records['androidLog']), 'errors': len(peer.records['errorLogMessage']), 'disk_records': len(disk), **result}
  finally:
    peer.close()


def fatal(binary, output, text):
  peer = Peer(output, binary)
  try:
    payload = text.encode() if isinstance(text, str) else text
    peer.send(payload + b'\n{}\n')
    result = peer.finish()
    assert result['exit'] != 0 and result['trace'][-1]['event'] == 'signal-15'
    assert peer.subscribers['androidLog'].receive(non_blocking=True) is None
    assert peer.subscribers['errorLogMessage'].receive(non_blocking=True) is None
    assert (output / 'journal.stderr').read_text()
    return result
  finally:
    peer.close()


def lifecycle(binary, output):
  results = {}
  for name, signum, command in [('sigint', signal.SIGINT, None), ('sigterm', signal.SIGTERM, None),
                                ('child-exit', None, {'op': 'exit', 'status': 17}),
                                ('empty-eof', None, {'op': 'close'})]:
    peer = Peer(output / name, binary)
    try:
      orphan = binary is None and signum == signal.SIGTERM
      result = peer.finish(command, signum, expect_orphan=orphan)
      if command is not None or binary is not None:
        assert result['exit'] == 0
      results[name] = result
    finally:
      peer.close()
  return results


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  binaries = [('original', None)] + ([] if args.binary is None else [('rust', args.binary.resolve())])
  results = {}
  for side, binary in binaries:
    records = {'normal': normal(binary, output / side / 'normal')}
    for index, text in enumerate(['[]', 'null', '{"_PID":null}', '{"_PID":"1.5"}', '{"_PID":2147483648}',
                                  '{"PRIORITY":256}', '{"PRIORITY":-1}', '{"__REALTIME_TIMESTAMP":-1}',
                                  '{"__REALTIME_TIMESTAMP":18446744073709551616}', '{"SYSLOG_IDENTIFIER":4}',
                                  '{"_PID":NaN}', '{"_PID":{}}', '{"_PID":"1__2"}',
                                  '{"SYSLOG_IDENTIFIER":"\\ud800"}', '{"_PID":"²"}',
                                  '{"_PID":"\\u001c1"}',
                                  '{"x":' + '1' * 4301 + '}', b'{"x":"\xff"}', b'{"x":"\xed\xa0\x80"}']):
      records[f'fatal-{index}'] = fatal(binary, output / side / f'fatal-{index}', text)
    records['lifecycle'] = lifecycle(binary, output / side / 'lifecycle')
    results[side] = records
    (output / 'report.json').write_text(json.dumps(results, indent=2))
  if args.binary is not None:
    assert results['original']['normal']['messages'] == results['rust']['normal']['messages']
  print(json.dumps(results, indent=2))


if __name__ == '__main__':
  main()
