#!/usr/bin/env python3
"""Real original/native beep processes on original msgq, fake PATH GPIO, and real overlap/timing."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time

import msgq
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from beep_fixtures import environment, lines


def scenario(binary, binding, output, mode, implementation):
  output.mkdir(parents=True, exist_ok=True)
  volume = b'bad' if mode == 'invalid-startup' else b'10'
  with tempfile.TemporaryDirectory(prefix='beep-live-') as temporary, environment(Path(temporary), binding, volume) as (config, params):
    root = Path(temporary)
    config['mode'] = 'live'
    if mode == 'status-failure':
      (root / 'status').write_text('7')
    if mode == 'missing-sudo':
      (root / 'bin/sudo').unlink()
    if mode == 'stop-startup':
      (root / 'hold-on').touch()
    shm = Path('/dev/shm/msgq_' + os.environ['OPENPILOT_PREFIX'])
    shm.mkdir()
    publisher = msgq.pub_sock('selfdriveState', SERVICE_LIST['selfdriveState'].queue_size)
    arguments = [str(binary)] if implementation == 'rust' else [sys.executable, str(Path(__file__).with_name('beep_source.py'))]
    if mode == 'bounded':
      arguments.extend(['--cycles', '3'])
    events, packets = [], []
    cadence = None
    process = None
    reader = None

    def commands(phase='end'):
      return [row for row in lines(root / 'commands.jsonl') if row['phase'] == phase]

    def wait_for(predicate, label, timeout=5):
      deadline = time.monotonic() + timeout
      while not predicate():
        assert time.monotonic() < deadline, (label, process.poll(), events, commands('start'))
        time.sleep(0.005)

    def read_stdout():
      for line in process.stdout:
        events.append({'time': time.monotonic(), 'text': line.rstrip()})

    def send(alert):
      before = len(events)
      message = log.Event.new_message()
      message.logMonoTime = time.monotonic_ns()
      message.valid = False  # The source reads updated alertSound regardless of outer validity.
      message.init('selfdriveState').alertSound = alert
      raw = message.to_bytes()
      (output / f'input-{len(packets)}.capnp').write_bytes(raw)
      packets.append({'alert': alert, 'valid': False, 'sent': time.monotonic()})
      publisher.send(raw)
      wait_for(lambda: any(row['text'] == f'[BEEP] New alert: {alert}' for row in events[before:]), 'new alert printed')

    try:
      with (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen(arguments, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
        process.stdin.write(json.dumps(config) + '\n')
        process.stdin.close()
        reader = threading.Thread(target=read_stdout, daemon=True)
        reader.start()
        if mode == 'bounded':
          code = process.wait(timeout=5)
          assert code == 0 and [row['value'] for row in commands()] == ['42', 'out', '1', '0']
          elapsed = None
        elif mode == 'invalid-startup':
          code = process.wait(timeout=5)
          assert code == (-6 if implementation == 'python' else 1), code
          assert [row['value'] for row in commands()] == ['42', 'out'], commands()
          elapsed = None
        elif mode == 'stop-startup':
          wait_for(lambda: len(list(root.glob('held-*'))) == 1, 'startup pulse held')
          started = time.monotonic()
          process.send_signal(signal.SIGINT)
          code = process.wait(timeout=2)
          elapsed = time.monotonic() - started
          assert code == (-signal.SIGINT if implementation == 'python' else 0) and elapsed < 1, (code, elapsed)
          (root / 'release').touch()
          wait_for(lambda: len(commands()) == 3, 'orphan fixture command completed')
          assert [row['value'] for row in commands()] == ['42', 'out', '1']
        else:
          if mode == 'missing-sudo':
            # Reader registration happens after the real 100ms startup sleep.
            publisher.wait_for_readers(timeout=5)
          else:
            wait_for(lambda: len(commands()) == 4, 'startup commands completed')
            startup = commands()
            assert [row['value'] for row in startup] == ['42', 'out', '1', '0'], startup
            starts = commands('start')
            gap = starts[3]['time'] - startup[2]['time']
            assert 0.09 < gap < 1, gap
            publisher.wait_for_readers(timeout=5)
          if mode in ['basic', 'status-failure']:
            for alert, pulses in [(1, 2), (2, 4), (3, 6), (10, 2), (15, 2), (24, 2)]:
              count = len(commands())
              send(alert)
              wait_for(lambda count=count, pulses=pulses: len(commands()) == count + pulses, 'alert pulse completion')
            count = len(commands())
            printed = len([row for row in events if row['text'].startswith('[BEEP]')])
            # An updated duplicate and an idle interval must not retrigger the current alert.
            message = log.Event.new_message()
            message.init('selfdriveState').alertSound = 24
            publisher.send(message.to_bytes())
            time.sleep(0.18)
            assert len(commands()) == count
            assert len([row for row in events if row['text'].startswith('[BEEP]')]) == printed
            for alert in [7, 8, 34, 36, 65535, 37, 0]:
              send(alert)
            assert len(commands()) == count
            if mode == 'status-failure':
              assert all(row['status'] == 7 for row in commands()), commands()
          elif mode == 'overlap-volume':
            (root / 'hold-on').touch()
            send(3)
            wait_for(lambda: len(list(root.glob('held-*'))) == 1, 'warning command held')
            send(1)
            wait_for(lambda: len(list(root.glob('held-*'))) == 2, 'engage overlaps warning')
            assert len(commands()) == 4, 'a held alert was serialized or prematurely released'
            send(0)  # None does not cancel either old worker.
            cadence_events = []
            for alert in [7, 8] * 5:
              send(alert)
              cadence_events.append(events[-1]['time'])
            intervals = [right - left for left, right in zip(cadence_events, cadence_events[1:], strict=False)]
            cadence = {'intervals': intervals, 'median_seconds': statistics.median(intervals)}
            assert 0.035 < cadence['median_seconds'] < 0.075, intervals
            (params / 'SoundVolumeAdjust').write_bytes(b'5')
            (root / 'release').touch()
            wait_for(lambda: len(commands()) == 12, 'both old alert workers finish after none/volume change')
            pulses = commands('start')[4:]
            assert [row['value'] for row in pulses[:2]] == ['1', '1'], pulses
            assert [row['value'] for row in pulses[2:]] == ['0'] * 6, pulses
          elif mode == 'invalid-worker':
            (params / 'SoundVolumeAdjust').write_bytes(b'bad')
            send(1)
            code = process.wait(timeout=5)
            assert code == (-6 if implementation == 'python' else 1), code
            assert len(commands()) == 4
          elif mode == 'stop-active':
            (root / 'hold-on').touch()
            send(3)
            wait_for(lambda: len(list(root.glob('held-*'))) == 1, 'worker command held')
          elif mode == 'missing-sudo':
            send(1)
            time.sleep(0.1)
            assert not commands()
          else:
            raise ValueError(mode)
          if mode != 'invalid-worker':
            started = time.monotonic()
            stop_signal = signal.SIGTERM if mode == 'stop-active' else signal.SIGINT
            process.send_signal(stop_signal)
            code = process.wait(timeout=2)
            elapsed = time.monotonic() - started
            assert code == (-stop_signal if implementation == 'python' else 0) and elapsed < 1, (code, elapsed)
            if mode == 'stop-active':
              (root / 'release').touch()
              wait_for(lambda: len(commands()) == 5, 'orphan fixture command completed')
              assert len(commands('start')) == 5, 'shutdown added an off pulse'
          else:
            elapsed = None
        reader.join(timeout=1)
        result = {'passed': True, 'mode': mode, 'implementation': implementation, 'exit_code': code,
                  'shutdown_elapsed': elapsed, 'cadence': cadence, 'events': events, 'packets': packets, 'commands': lines(root / 'commands.jsonl')}
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        (output / 'stdout.log').write_text('\n'.join(row['text'] for row in events) + '\n')
        return result
    finally:
      (root / 'release').touch()
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=3)
      if reader is not None:
        reader.join(timeout=1)
      publisher = None
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--worker', nargs=2)
  args = parser.parse_args()
  if args.worker:
    scenario(args.binary.resolve(), args.binding.resolve(), args.output.resolve(), *args.worker)
    return
  args.output.mkdir(parents=True, exist_ok=True)
  modes = ['basic', 'status-failure', 'overlap-volume', 'invalid-startup', 'invalid-worker', 'stop-active', 'stop-startup', 'missing-sudo']

  def run(row):
    mode, implementation = row
    output = args.output / f'{mode}-{implementation}'
    subprocess.run([sys.executable, __file__, '--binary', str(args.binary.resolve()), '--binding', str(args.binding.resolve()),
                    '--output', str(output), '--worker', mode, implementation], check=True)
    return json.loads((output / 'result.json').read_text())

  with ThreadPoolExecutor(max_workers=4) as pool:
    cases = [(mode, implementation) for mode in modes for implementation in ['python', 'rust']] + [('bounded', 'rust')]
    results = list(pool.map(run, cases))
  cli = []
  with tempfile.TemporaryDirectory(prefix='beep-cli-') as temporary, environment(Path(temporary), args.binding.resolve()):
    for arguments, expected in [(['--help'], 0), (['--cycles', '0'], 1), (['--unknown'], 1), (['--cycles', '1', 'extra'], 1)]:
      result = subprocess.run([str(args.binary.resolve()), *arguments], capture_output=True, text=True, timeout=3)
      assert result.returncode == expected and not lines(Path(temporary) / 'commands.jsonl'), (arguments, result)
      cli.append({'arguments': arguments, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
  (args.output / 'cli.json').write_text(json.dumps(cli, indent=2) + '\n')
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'executions': len(results), 'results': results}, indent=2) + '\n')
  print(f'PASS: {len(results)} real original/native msgq, GPIO-command fixture, overlap, volume, fatal error and shutdown executions')


if __name__ == '__main__':
  main()
