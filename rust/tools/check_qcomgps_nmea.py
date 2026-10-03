#!/usr/bin/env python3
"""NMEA source quirks, typed values, and native private-PTY reopen/shutdown checks."""
import argparse
from contextlib import redirect_stdout
from dataclasses import dataclass, fields
import io
import json
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import tempfile
import time
import tty

from qcomgps_reference import definitions
from qcomgps_peer import Peer


def reference(line):
  scope = {'__name__': __name__, 'dataclass': dataclass, 'fields': fields, 'os': os, 'NoReturn': object}
  definitions('nmeaport.py', scope)
  line = line.strip()
  with redirect_stdout(io.StringIO()):
    if not line.startswith('$') or not scope['nmea_checksum_ok'](line):
      return {'ignored': True}
  parts = line.split(',')
  name, stop = {'$GNCLK': ('GnssClockNmeaPort', 10), '$GNMEAS': ('GnssMeasNmeaPort', 14)}.get(parts[0], (None, 0))
  if name is None:
    return {'ignored': True}
  try:
    return {'repr': repr(scope[name](*parts[1:stop]))}
  except (ValueError, TypeError):
    return {'error': True}


def comparisons(binary, output):
  lines = ['$GNCLK,1,18,0,0,-100,0,0,0,0,,*ZZ', '$GNCLK,,,-100,,999999999999999999999999,1.5,-0.0,inf,nan,,*12',
           '$GNMEAS,2,1,5,3,7,0,-100,4,987654321,2,34.5,-10.2,0.1,,*FF', '$GNCLK,1*00',
           '$GNCLK,1,18,0,0,-100,0,0,0,no,,*00', '$GNCLK,1,18,0,0,-100,0,0,0,0,,*0',
           '$GNCLK,1,18,0,0,-100,0,0,0,0,,*000', '$GNGGA,,,,*00', 'ignored', '*12', '$']
  completed = subprocess.run([binary], input='\n'.join(lines) + '\n', capture_output=True, text=True, check=True, timeout=5)
  results = [json.loads(line) for line in completed.stdout.splitlines()]
  records = []
  for line, actual in zip(lines, results, strict=True):
    expected = reference(line)
    assert ('error' in actual) if 'error' in expected else actual == expected, (line, actual, expected)
    records.append({'line': line, 'source': expected, 'native': actual, 'pass': True})
  # This is the actual source bytes-membership failure on at_cmd's str result.
  try:
    b'+QGPS: 0' not in ('+QGPS: 0' or b'')
  except TypeError as error:
    records.append({'source_setup_error': str(error)})
  output.write_text(json.dumps(records, indent=2) + '\n')


def runtime(binary, output):
  with tempfile.TemporaryDirectory(prefix='qcom-nmea-') as temporary:
    root = Path(temporary)
    master, slave = pty.openpty()
    tty.setraw(slave)
    path = root / 'nmea'
    process = subprocess.Popen([binary, '--read', path], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
    records = []
    try:
      assert select.select([process.stdout], [], [], 3)[0]
      records.append(process.stdout.readline().strip())
      assert 'No such file' in records[-1]
      path.symlink_to(os.ttyname(slave))
      time.sleep(1.2)
      good = '$GNCLK,1,18,0,0,-100,0,0,0,0,,*ZZ'
      os.write(master, (good + '\r\n').encode())
      assert select.select([process.stdout], [], [], 3)[0]
      records.append(process.stdout.readline().strip())
      assert records[-1] == reference(good)['repr']
      os.write(master, b'$GNMEAS,1*00\r\n')
      assert select.select([process.stdout], [], [], 3)[0]
      records.append(process.stdout.readline().strip())
      assert 'missing positional field' in records[-1]
      time.sleep(1.2)
      os.write(master, (good + '\n').encode())
      assert select.select([process.stdout], [], [], 3)[0]
      records.append(process.stdout.readline().strip())
      assert records[-1] == reference(good)['repr']
      process.send_signal(signal.SIGTERM)
      assert process.wait(timeout=3) == 0
      output.write_text(json.dumps({'observed': records, 'returncode': process.returncode, 'pass': True}, indent=2) + '\n')
    finally:
      if process.poll() is None:
        process.kill()
      process.wait(timeout=3)
      os.close(master)
      os.close(slave)


def setup_cases(binary, output):
  records = []
  for empty in [False, True]:
    with tempfile.TemporaryDirectory(prefix='qcom-nmea-setup-') as temporary:
      root = Path(temporary)
      peer = Peer(root)
      peer.empty_query = empty
      config = root / 'config.json'
      config.write_text(json.dumps(peer.config))
      try:
        result = subprocess.run([binary, '--fixture', config], text=True, capture_output=True, timeout=5)
        assert result.returncode == (2 if empty else 1), (result.stdout, result.stderr)
        if empty:
          assert peer.at[-1] == 'AT+CFUN=1,1'
          assert 're-run this script' in result.stdout
        else:
          assert peer.at == ['AT+QGPS?']
          assert "requires string as left operand, not bytes" in result.stderr
        assert (root / 'sys/class/gpio/gpio34/value').read_text() == '1'
        records.append({'empty_reply': empty, 'at': peer.at, 'stdout': result.stdout, 'stderr': result.stderr,
                        'returncode': result.returncode, 'pass': True})
      finally:
        peer.close()
  output.write_text(json.dumps(records, indent=2) + '\n')


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('trace', type=Path)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  comparisons(args.trace, args.output / 'reference.json')
  runtime(args.binary, args.output / 'runtime.json')
  setup_cases(args.binary, args.output / 'setup.json')
  print('PASS: NMEA source values/checksum quirks and private PTY missing-open/parse-error/reopen/signal lifecycle')


if __name__ == '__main__':
  main()
