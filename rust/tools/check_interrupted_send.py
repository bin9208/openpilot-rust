#!/usr/bin/env python3
"""Compare EINTR/EAGAIN/fatal libzmq sends in actual Python and Rust producers."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile

import zmq

ROOT = Path(__file__).resolve().parents[2]
CASES = {'one-interrupt': '4,0', 'repeated-interrupt': '4,4,4,0', 'backpressure': '4,11', 'invalid': '4,22', 'closed': '4,88'}


def response(process):
  assert select.select([process.stdout], [], [], 5)[0], 'child response timeout'
  return process.stdout.readline().strip()


def trace_rows(path):
  rows = []
  for line in path.read_text().splitlines():
    columns = line.split('\t')
    rows.append(dict(zip(('pid', 'tid', 'attempt', 'injected', 'result', 'errno', 'flags', 'length'), map(int, columns[:8])), packet=columns[8]))
  return rows


def scenario(args, kind, case, source):
  directory = args.output / kind / case / ('source' if source else 'native')
  directory.mkdir(parents=True)
  trace = directory / 'send.tsv'
  environment = dict(os.environ, ZMQ_TEST_SEND_MATCH='eintr-probe', ZMQ_TEST_SEND_ERRORS=CASES[case], ZMQ_TEST_SEND_TRACE=str(trace))
  environment.pop('LOGPRINT', None)
  if source:
    environment.update(LD_PRELOAD=str(args.preload), ZMQ_TEST_REAL_LIBRARY=str(args.python_zmq))
  else:
    environment.pop('LD_PRELOAD', None)
  with tempfile.TemporaryDirectory(prefix='eintr-') as temporary, zmq.Context() as context:
    socket = context.socket(zmq.PULL)
    socket.linger = 0
    endpoint = 'ipc://' + temporary + '/pull'
    socket.bind(endpoint)
    command = [sys.executable, str(ROOT / 'rust/tools/interrupted_send_reference.py'), kind, endpoint] if source else [str(getattr(args, kind)), endpoint]
    (directory / 'command.json').write_text(json.dumps({'argv': command, 'sequence': CASES[case]}, indent=2) + '\n')
    with (directory / 'stderr.log').open('w') as stderr:
      process = subprocess.Popen(command, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1)
      try:
        def send(text):
          request = json.dumps(text) if source else json.dumps({'action': 'emit', 'level': 40, 'text': text}) if kind == 'logging' else text
          process.stdin.write(request + '\n')
          process.stdin.flush()
          return response(process)
        warmup = send('warmup')
        assert socket.poll(3000), ('warmup delivery', warmup)
        socket.recv()
        actual = send('eintr-probe')
        process.stdin.close()
        status = process.wait(timeout=5)
        packets = []
        while socket.poll(100):
          packets.append(socket.recv())
        for index, packet in enumerate(packets):
          (directory / f'packet-{index}.bin').write_bytes(packet)
      finally:
        if process.poll() is None:
          process.kill()
          process.wait(timeout=5)
        socket.close()
  rows = trace_rows(trace)
  before = args.before and not source
  faults = [int(value) for value in CASES[case].split(',')]
  expected = [4] if before else faults
  assert [row['errno'] for row in rows] == expected, rows
  assert [row['attempt'] for row in rows] == list(range(len(expected))), rows
  assert all(row['flags'] == zmq.DONTWAIT for row in rows)
  assert len({row['packet'] for row in rows}) == 1, 'retry changed packet'
  assert all(len(bytes.fromhex(row['packet'])) == row['length'] for row in rows), 'truncated capture'
  terminal = expected[-1]
  assert len(packets) == (1 if terminal == 0 else 0), ('delivery count', packets)
  if packets:
    assert packets[0].hex() == rows[-1]['packet']
  for row in rows:
    assert row['result'] == (-1 if row['errno'] else row['length'])
    assert row['injected'] == bool(row['errno'])
  if source:
    result = json.loads(actual)
    assert result == {'ok': True} if terminal in (0, 11) else result['errno'] == terminal
    assert status == 0
  elif kind == 'logging':
    result = json.loads(actual)
    assert result == {'delivery': 'sent' if terminal == 0 else 'dropped'} if terminal in (0, 11) else 'error' in result
    assert status == 0
  else:
    assert actual == ('Sent' if terminal == 0 else 'Dropped') if terminal in (0, 11) else actual == ''
    assert status == (0 if terminal in (0, 11) else 1)
  console = (directory / 'stderr.log').read_text()
  if kind == 'logging':
    assert console.splitlines().count('eintr-probe') == 1, console
  report = {'result': 'pass', 'before_native': before, 'response': actual, 'exit': status, 'attempts': len(rows), 'delivered': len(packets), 'terminal_errno': terminal}
  (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  for option in ('logging', 'stats', 'preload', 'python-zmq', 'output'):
    parser.add_argument('--' + option, type=Path, required=True)
  parser.add_argument('--before', action='store_true')
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  reports = {f'{kind}/{case}/{source}': scenario(args, kind, case, source) for kind in ('logging', 'stats') for case in CASES for source in (True, False)}
  sources = ['rust/crates/logging/src/producer.rs', 'rust/crates/statsd/src/producer.rs', 'openpilot/common/swaglog.py', 'openpilot/common/logging_extra.py', 'openpilot/system/statsd.py', 'rust/tools/zmq_send_boundary.c', 'rust/tools/interrupted_send_reference.py', 'rust/tools/check_interrupted_send.py']
  paths = [ROOT / path for path in sources] + [args.logging, args.stats, args.preload, args.python_zmq]
  report = {'result': 'pass', 'cases': reports, 'python': sys.version, 'pyzmq': zmq.__version__, 'libzmq': zmq.zmq_version(), 'sha256': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}}
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'result': 'pass', 'cases': len(reports), 'before_native': args.before}))


if __name__ == '__main__':
  main()
