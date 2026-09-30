#!/usr/bin/env python3
"""Drive extreme GPS dates through actual original/native loops with safe command fixtures."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from check_timed_reference import step
from timed_fixtures import commands, environment
from timed_reference import Source


def worker(binary, output, zone, milliseconds, implementation):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='timed-date-live-') as temporary, environment(Path(temporary), zone) as (config, params):
    (params / 'TimezoneSource').write_text('app')
    source = Source(config, params)
    expected_error = None
    try:
      source.loop([step(10_000_000_000, unix_timestamp_millis=milliseconds)])
    except (ValueError, OverflowError, OSError) as error:
      expected_error = str(error)
    config.update(live=True, monotonic=None)
    shm = Path('/dev/shm/msgq_' + os.environ['OPENPILOT_PREFIX'])
    shm.mkdir()
    publisher = msgq.pub_sock('gpsLocation', SERVICE_LIST['gpsLocation'].queue_size)
    receiver = msgq.sub_sock('clocks', conflate=False, timeout=2500, segment_size=SERVICE_LIST['clocks'].queue_size)
    process = None
    try:
      arguments = [str(binary)] if implementation == 'rust' else [sys.executable, str(Path(__file__).with_name('timed_source_live.py'))]
      with (output / 'process.log').open('w') as stderr:
        process = subprocess.Popen(arguments, stdin=subprocess.PIPE, stdout=stderr, stderr=stderr, text=True)
        process.stdin.write(json.dumps(config) + '\n')
        process.stdin.close()
        publisher.wait_for_readers(timeout=5)
        packets = []

        def receive():
          raw = receiver.receive()
          if raw is None:
            return False
          with log.Event.from_bytes(raw) as message:
            assert message.which() == 'clocks'
          (output / f'clocks-{len(packets)}.capnp').write_bytes(raw)
          packets.append(len(raw))
          return True

        assert receive(), ('initial clocks absent', process.poll())
        event = log.Event.new_message()
        event.logMonoTime = time.monotonic_ns()
        event.valid = True
        gps = event.init('gpsLocation')
        gps.hasFix = True
        gps.unixTimestampMillis = milliseconds
        raw = event.to_bytes()
        (output / 'gps.capnp').write_bytes(raw)
        publisher.send(raw)
        if expected_error is not None:
          try:
            code = process.wait(timeout=4)
          except subprocess.TimeoutExpired:
            process.kill()
            code = process.wait(timeout=3)
          passed = code == 1
        else:
          continued = receive() and receive() and process.poll() is None
          process.send_signal(signal.SIGINT)
          code = process.wait(timeout=4)
          passed = continued and code == (0 if implementation == 'rust' else -signal.SIGINT)
        result = {'zone': zone, 'unix_timestamp_millis': milliseconds, 'implementation': implementation,
                  'source_error': expected_error, 'exit_code': code, 'clock_packet_sizes': packets,
                  'commands': commands(config), 'passed': passed and not commands(config)}
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        return result
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      publisher = receiver = None
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--worker', nargs=3)
  args = parser.parse_args()
  if args.worker:
    zone, milliseconds, implementation = args.worker
    worker(args.binary, args.output, zone, int(milliseconds), implementation)
    return
  args.output.mkdir(parents=True, exist_ok=True)
  cases = [('UTC', 253402300800000), ('UTC', 253402300799999), ('Etc/GMT+12', 253402300800000),
           ('Etc/GMT-14', 253402250400000), ('Etc/GMT-14', -62135596800001),
           ('Etc/GMT-14', -62135560800000), ('Etc/GMT+12', -62135467200000), ('UTC', -62135596800000)]

  def run(row):
    zone, milliseconds, implementation = row
    output = args.output / (zone.replace('/', '-') + '-' + str(milliseconds) + '-' + implementation)
    subprocess.run([sys.executable, __file__, '--binary', str(args.binary), '--output', str(output),
                    '--worker', zone, str(milliseconds), implementation], check=True)
    return json.loads((output / 'result.json').read_text())

  with ThreadPoolExecutor(max_workers=4) as pool:
    results = list(pool.map(run, [(zone, milliseconds, implementation) for zone, milliseconds in cases for implementation in ['python', 'rust']]))
  failures = [result for result in results if not result['passed']]
  (args.output / 'summary.json').write_text(json.dumps({'passed': not failures, 'executions': len(results), 'results': results}, indent=2) + '\n')
  print(json.dumps({'passed': not failures, 'executions': len(results), 'failed_cases': len(failures)}, indent=2))
  assert not failures, failures


if __name__ == '__main__':
  main()
