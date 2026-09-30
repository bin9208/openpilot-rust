#!/usr/bin/env python3
"""Continuous native sensord: owned ioctl peers, isolated real msgq/logging, recoverable errors and SIGINT."""

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import uuid

import zmq


def wait_for(predicate, description, timeout=8):
  deadline = time.monotonic() + timeout
  while time.monotonic() < deadline:
    value = predicate()
    if value:
      return value
    time.sleep(0.02)
  raise AssertionError('timeout: ' + description)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.target = args.target.resolve()
  args.evidence.mkdir(parents=True, exist_ok=True)
  fixture = args.evidence / 'sensord-kernel-fixture.so'
  subprocess.run(
    ['clang', '-shared', '-fPIC', '-std=gnu11', '-O1', str(Path(__file__).with_name('sensord_kernel_fixture.c')), '-ldl', '-pthread', '-o', fixture], check=True
  )
  prefix = 'rust-probe-sensord-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  context = zmq.Context()
  logs = context.socket(zmq.PULL)
  logs.setsockopt(zmq.LINGER, 0)
  logs.bind('ipc:///tmp/logmessage' + prefix)
  packets, records = [], []
  processes, threads = [], []
  with tempfile.TemporaryDirectory(prefix='sensord-daemon-') as temporary:
    root = Path(temporary)
    for name in ('TICI', 'dev/i2c-1', 'dev/gpiochip0', 'proc/irq/336/smp_affinity_list'):
      path = root / name
      path.parent.mkdir(parents=True, exist_ok=True)
      path.write_text('0\n')
    trace, state = args.evidence / 'daemon-kernel.jsonl', args.evidence / 'daemon-kernel-state.json'
    trace.unlink(missing_ok=True)
    environment = os.environ | {'OPENPILOT_PREFIX': prefix}
    for key in ('ZMQ', 'CEREAL_FAKE', 'LSM_SELF_TEST'):
      environment.pop(key, None)
    ready = threading.Event()
    try:
      with (args.evidence / 'receiver.stderr').open('w') as error:
        receiver = subprocess.Popen([args.target / 'debug/examples/sensord_receive', '12'], stdout=subprocess.PIPE, stderr=error, env=environment, text=True)
        processes.append(receiver)

        def collect():
          with (args.evidence / 'packets.jsonl').open('w') as capture:
            for line in receiver.stdout:
              capture.write(line)
              capture.flush()
              message = json.loads(line)
              if message.get('ready'):
                ready.set()
              else:
                packets.append(message)

        reader = threading.Thread(target=collect, daemon=True)
        reader.start()
        threads.append(reader)
        assert ready.wait(5), 'subscriber did not open'
        daemon_env = environment | {
          'LD_PRELOAD': str(fixture.resolve()),
          'SENSORD_CONTINUOUS': '1',
          'SENSORD_I2C': str(root / 'dev/i2c-1'),
          'SENSORD_GPIO': str(root / 'dev/gpiochip0'),
          'SENSORD_TRACE': str(trace.resolve()),
          'SENSORD_STATE': str(state.resolve()),
          'SENSORD_FAULT': str(root / 'fault'),
          'PATH': str(root / 'no-commands'),
        }
        started = time.monotonic()
        with (args.evidence / 'daemon.stderr').open('w') as stderr:
          daemon = subprocess.Popen([args.target / 'debug/openpilot-sensord', '--root', root], env=daemon_env, stdout=stderr, stderr=stderr)
          processes.append(daemon)
          native_exe = str(Path(f'/proc/{daemon.pid}/exe').resolve())
          assert native_exe == str(args.target / 'debug/openpilot-sensord')
          assert os.sched_getscheduler(daemon.pid) == os.SCHED_OTHER
          assert os.sched_getaffinity(daemon.pid) == os.sched_getaffinity(0)
          wait_for(lambda: any(p['service'] == 'temperatureSensor' for p in packets) and len(packets) > 50, 'settled native publications')
          first_age = packets[0]['logMonoTime'] / 1e9 - started
          assert first_age > 0.6, first_age
          (root / 'fault').touch()

          def error_received():
            while logs.poll(0):
              records.append(json.loads(logs.recv()[1:]))
            return any(row['msg'] == 'Error processing accelerometer' and row.get('exc_info') for row in records)

          wait_for(error_received, 'native read error log')
          during = Counter(p['service'] for p in packets)
          time.sleep(0.2)
          after = Counter(p['service'] for p in packets)
          assert after['accelerometer'] - during['accelerometer'] <= 2
          (root / 'fault').unlink()
          baseline = len(packets)
          wait_for(lambda: len(packets) >= baseline + 30, 'publication recovery')
          wait_for(lambda: sum(p['service'] == 'temperatureSensor' for p in packets) >= 3, '2 Hz temperature publications')
          stop_started = time.monotonic()
          daemon.send_signal(signal.SIGINT)
          assert daemon.wait(timeout=4) == 0
          stop_time = time.monotonic() - stop_started
          error_received()
        receiver.send_signal(signal.SIGTERM)
        receiver.wait(timeout=3)
        reader.join(timeout=3)
      for packet in packets:
        assert packet['valid'] and packet['event']['source'] == 'lsm6ds3trc'
        assert packet['event']['deprecated'] == {'version': 0, 'sensor': 0, 'type': 0, 'uncalibrated': False}
        assert abs(packet['logMonoTime'] - packet['event']['timestamp']) < 25_000_000
      temperatures = [p for p in packets if p['service'] == 'temperatureSensor']
      intervals = [(b['event']['timestamp'] - a['event']['timestamp']) / 1e9 for a, b in zip(temperatures, temperatures[1:], strict=False)]
      assert all(0.3 < period < 0.8 for period in intervals), intervals
      assert all(p['event']['temperature'] == 24.0 for p in temperatures)
      kernel = json.loads(state.read_text())
      assert kernel['int1'] == 0x80 and kernel['ctrl1'] == 0 and kernel['ctrl2'] == 0
      assert kernel['reads'] > 200 and kernel['irq_events'] > 50
      calls = [json.loads(line) for line in trace.read_text().splitlines()]
      assert {'kind': 'scheduler', 'policy': 1, 'priority': 1} in calls
      assert {'kind': 'affinity', 'core': 1} in calls
      assert sum(c.get('device') == 'i2c' and c.get('closed') is True for c in calls) == 3
      assert any(c.get('device') == 'event' and c.get('closed') is True for c in calls)
      assert (root / 'proc/irq/336/smp_affinity_list').read_text() == '1\n'
      result = {
        'exe': native_exe,
        'counts': dict(Counter(p['service'] for p in packets)),
        'first_publication_age': first_age,
        'temperature_intervals': intervals,
        'stop_seconds': stop_time,
        'exit_code': daemon.returncode,
        'fault_logged_and_recovered': True,
        'actual_scheduler_unchanged': True,
        'owned_irq_affinity_written': True,
        'kernel': kernel,
      }
      (args.evidence / 'daemon-results.json').write_text(json.dumps(result, indent=2) + '\n')
      (args.evidence / 'daemon-logs.json').write_text(json.dumps(records, indent=2) + '\n')
      print(json.dumps(result))
    finally:
      for process in reversed(processes):
        if process.poll() is None:
          process.kill()
          process.wait()
      for thread in threads:
        thread.join(timeout=2)
      logs.close()
      context.term()
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)


if __name__ == '__main__':
  main()
