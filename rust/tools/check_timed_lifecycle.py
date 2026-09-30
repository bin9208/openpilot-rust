#!/usr/bin/env python3
"""Exercise production entrypoint safely: app timezone, no GPS publisher, fixture PATH."""
import argparse
import json
from pathlib import Path
import os
import shutil
import signal
import subprocess
import tempfile
import time

import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from timed_fixtures import environment, commands


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  results = []
  for mode in ['bounded', 'sigint', 'sigterm']:
    with tempfile.TemporaryDirectory(prefix='timed-entry-') as temporary, environment(Path(temporary)) as (config, params):
      (params / 'TimezoneSource').write_text('app')
      shm = Path('/dev/shm/msgq_' + os.environ['OPENPILOT_PREFIX'])
      shm.mkdir()
      publisher = msgq.pub_sock('gpsLocation', SERVICE_LIST['gpsLocation'].queue_size)
      receiver = msgq.sub_sock('clocks', conflate=False, timeout=5000, segment_size=SERVICE_LIST['clocks'].queue_size)
      process = None
      try:
        with (args.output / (mode + '.log')).open('w') as stderr:
          process = subprocess.Popen([str(args.binary), *(['--cycles', '2'] if mode == 'bounded' else [])], stdout=stderr, stderr=stderr)
          publisher.wait_for_readers(timeout=5)
          raw = receiver.receive()
          assert raw is not None, (mode, process.poll())
          (args.output / (mode + '.capnp')).write_bytes(raw)
          with log.Event.from_bytes(raw) as packet:
            assert packet.which() == 'clocks'
            assert abs(packet.clocks.wallTimeNanos / 1e9 - time.time_ns() / 1e9) < 3
            assert 0 <= time.monotonic() - packet.logMonoTime / 1e9 < 3
          if mode != 'bounded':
            process.send_signal(signal.SIGINT if mode == 'sigint' else signal.SIGTERM)
          code = process.wait(timeout=5)
          assert code == 0 and not commands(config), (mode, code, commands(config))
          results.append({'mode': mode, 'exit_code': code, 'commands': commands(config), 'packet_bytes': len(raw)})
      finally:
        if process is not None and process.poll() is None:
          process.kill()
          process.wait(timeout=3)
        del receiver, publisher
        shutil.rmtree(shm)
  for arguments, expected in [(['--help'], 0), (['--cycles', '0'], 1), (['--unknown'], 1), (['--cycles', '1', 'extra'], 1)]:
    result = subprocess.run([str(args.binary), *arguments], capture_output=True, text=True, timeout=5)
    assert result.returncode == expected, (arguments, result)
    results.append({'arguments': arguments, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'executions': len(results), 'results': results}, indent=2) + '\n')
  print('PASS: real timed entrypoint publication, bounded exit, SIGINT/SIGTERM and CLI errors; zero clock commands')


if __name__ == '__main__':
  main()
