#!/usr/bin/env python3
"""Native msgq, real PUSH logs and PATH-selected command fixtures; never touches a vehicle."""
import argparse
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
import zmq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from timed_fixtures import environment, commands


def scenario(binary, output, external, implementation):
  output.mkdir(parents=True, exist_ok=True)
  with tempfile.TemporaryDirectory(prefix='timed-daemon-') as temporary, environment(Path(temporary)) as (config, params):
    (params / 'TimezoneSource').write_text('app')
    (params / 'UbloxAvailable').write_text('1' if external else '0')
    config.update(live=True, monotonic=None)
    prefix = os.environ['OPENPILOT_PREFIX']
    shm = Path('/dev/shm/msgq_' + prefix)
    shm.mkdir()
    service = 'gpsLocationExternal' if external else 'gpsLocation'
    publisher = msgq.pub_sock(service, SERVICE_LIST[service].queue_size)
    receiver = msgq.sub_sock('clocks', conflate=False, timeout=1500, segment_size=SERVICE_LIST['clocks'].queue_size)
    context = zmq.Context()
    collector = context.socket(zmq.PULL)
    collector.setsockopt(zmq.LINGER, 0)
    collector.bind('ipc:///tmp/logmessage' + prefix)
    args = [str(binary)] if implementation == 'rust' else [sys.executable, str(Path(__file__).with_name('timed_source_live.py'))]
    packets, records = [], []
    process = None
    started = time.monotonic()
    try:
      with (output / 'daemon.log').open('w') as stderr:
        process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=stderr, stderr=stderr, text=True)
        process.stdin.write(json.dumps(config) + '\n')
        process.stdin.close()
        publisher.wait_for_readers(timeout=5)
        assert process.poll() is None, (output / 'daemon.log').read_text()

        def receive():
          raw = receiver.receive()
          assert raw is not None, ('missing clocks', process.poll(), (output / 'daemon.log').read_text())
          (output / f'clocks-{len(packets)}.capnp').write_bytes(raw)
          with log.Event.from_bytes(raw) as packet:
            assert packet.which() == 'clocks' and packet.valid and packet.clocks.wallTimeNanos == config['wall']
            age = time.monotonic() - packet.logMonoTime / 1e9
            assert 0 <= age < 1.5, age
            packets.append({'time': time.monotonic(), 'valid': packet.valid, 'wall': packet.clocks.wallTimeNanos, 'mono': packet.logMonoTime})

        def send(age=0.0, fix=True, diff=0):
          packet = log.Event.new_message()
          packet.logMonoTime = time.monotonic_ns() - int(age * 1e9)
          packet.valid = False  # Source ignores outer validity for GPS set-time decisions.
          gps = packet.init(service)
          gps.hasFix = fix
          gps.longitude = 127.0
          gps.unixTimestampMillis = config['wall'] // 1000000 + diff * 1000
          raw = packet.to_bytes()
          (output / f'gps-{len(packets)}.capnp').write_bytes(raw)
          publisher.send(raw)

        def record():
          assert collector.poll(3000), ('missing log', process.poll(), (output / 'daemon.log').read_text())
          raw = collector.recv()
          row = json.loads(raw[1:])
          assert raw[0] == row['levelnum'] and row['name'] == 'swaglog'
          assert row['pathname'] and row['lineno'] > 0 and row['funcName']
          records.append(row)
          return row

        receive()  # Continuous timed publication without a GPS update.
        send(age=3, diff=30)
        receive()
        assert not commands(config)
        send(fix=False, diff=30)
        receive()
        assert not commands(config)
        send(diff=0)
        receive()
        sleep_start = time.monotonic()
        assert record()['msg'] == 'Time diff too small: 0.0s'
        assert not commands(config)
        assert receiver.receive() is None, 'loop failed to retain the ten-second sleep'
        while time.monotonic() - sleep_start < 9.6:
          time.sleep(0.05)
        send(diff=30)
        receive()
        sleep_elapsed = time.monotonic() - sleep_start
        assert 9.8 < sleep_elapsed < 12, sleep_elapsed
        assert record()['msg'] == 'Setting system time from GPS (diff 30.0s)'
        deadline = time.monotonic() + 3
        while not commands(config):
          assert time.monotonic() < deadline
          time.sleep(0.01)
        assert commands(config) == [['date', '-s', '@1790000030']]
        assert not (params / 'TimezoneName').exists(), 'app timezone was overwritten'
        process.send_signal(signal.SIGTERM)
        code = process.wait(timeout=12)
        assert code == (0 if implementation == 'rust' else -signal.SIGTERM), code
        result = {'passed': True, 'implementation': implementation, 'service': service, 'packets': packets, 'records': records,
                  'commands': commands(config), 'sleep_elapsed': sleep_elapsed, 'exit_code': code, 'elapsed': time.monotonic() - started}
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        return result
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      collector.close()
      context.term()
      publisher = receiver = None
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  results = []
  for external in [False, True]:
    for implementation in ['python', 'rust']:
      results.append(scenario(args.binary, args.output / f'{implementation}-{external}', external, implementation))
  summary = {'passed': True, 'executions': len(results), 'services': [r['service'] for r in results]}
  (args.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
  print('PASS: original and native continuous msgq loops, both GPS topics, real logs, ten-second sleep, command fixtures')


if __name__ == '__main__':
  main()
