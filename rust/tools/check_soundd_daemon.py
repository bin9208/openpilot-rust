#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# How to run: use the commands and original-binding environment in
# docs/rust-port/soundd-validation.md; never initialize a real audio device.
"""Actual native soundd through owned PortAudio ABI and original isolated msgq."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import statistics
import subprocess
import tempfile
import time
import uuid

import msgq
import numpy as np
from openpilot.cereal import log
from openpilot.cereal.services import SERVICE_LIST
from check_soundd import assets


def lines(path):
  if not path.exists():
    return []
  return [json.loads(line) for line in path.read_text().splitlines()]


def scenario(binary, library, output, mode):
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'sound-' + uuid.uuid4().hex[:16]
  old = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  process = None
  publishers = {}
  try:
    with tempfile.TemporaryDirectory(prefix='sound-native-') as temporary:
      root = Path(temporary)
      base = assets(root, 48000, 1)
      params = root / 'params' / prefix
      params.mkdir(parents=True)
      for key, value in [('SoundVolumeAdjustEngage', '37'), ('SoundVolumeAdjust', '73'), ('SoundLanguageSetting', 'en')]:
        (params / key).write_text(value)
      env = {
        **os.environ,
        'PARAMS_ROOT': str(root / 'params'),
        'SOUND_FIXTURE_EVENTS': str(output / 'events.jsonl'),
        'SOUND_FIXTURE_SAMPLES': str(output / 'samples.f32'),
        'LOGPRINT': 'info',
      }
      match mode:
        case 'retry':
          env['SOUND_FIXTURE_FAIL_OPEN'] = '2'
        case 'exhausted':
          env['SOUND_FIXTURE_FAIL_OPEN'] = '10'
        case 'inactive':
          env['SOUND_FIXTURE_INACTIVE'] = '1'
        case 'start-failed':
          env['SOUND_FIXTURE_FAIL_START'] = '1'
        case 'stop-retry':
          env['SOUND_FIXTURE_FAIL_OPEN'] = '10'
        case 'live' | 'bounded':
          pass
        case _:
          raise ValueError(mode)
      for topic in ['selfdriveState', 'carrotMan', 'soundPressure', 'carState']:
        publishers[topic] = msgq.pub_sock(topic, SERVICE_LIST[topic].queue_size)
      arguments = [str(binary), '--assets', str(base), '--portaudio-library', str(library)]
      if mode in ['bounded', 'retry']:
        arguments.extend(['--cycles', '5'])

      def wait_for(predicate, timeout=5):
        deadline = time.monotonic() + timeout
        while not predicate():
          assert time.monotonic() < deadline, (mode, process.poll(), lines(output / 'events.jsonl'))
          time.sleep(0.005)

      def event_count(name):
        return sum(row['event'] == name for row in lines(output / 'events.jsonl'))

      packets = []

      def send(topic, **values):
        event = log.Event.new_message()
        event.logMonoTime = time.monotonic_ns()
        event.valid = False
        message = event.init(topic)
        for key, value in values.items():
          setattr(message, key, value)
        raw = event.to_bytes()
        (output / f'input-{len(packets)}.capnp').write_bytes(raw)
        publishers[topic].send(raw)
        packets.append({'topic': topic, 'values': values, 'time': time.monotonic()})

      with (output / 'stdout.log').open('w') as stdout, (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen(arguments, env=env, stdout=stdout, stderr=stderr)
        if mode == 'stop-retry':
          wait_for(lambda: event_count('open') == 1)
          started = time.monotonic()
          process.send_signal(signal.SIGTERM)
          code = process.wait(timeout=2)
          assert code == 0 and time.monotonic() - started < 1
        elif mode in ['exhausted', 'inactive', 'start-failed']:
          code = process.wait(timeout=35)
          assert code == 1, code
        elif mode in ['bounded', 'retry']:
          code = process.wait(timeout=10)
          assert code == 0, code
        else:
          for publisher in publishers.values():
            publisher.wait_for_readers(timeout=5)
          wait_for(lambda: event_count('callback') >= 1)
          send('soundPressure', soundPressureWeightedDb=54.0)
          wait_for(lambda: event_count('poll') >= 3)
          send('selfdriveState', enabled=False, alertSound=1)
          wait_for(lambda: event_count('callback') >= 5)
          samples = np.fromfile(output / 'samples.f32', dtype=np.float32).reshape(-1, 4096)
          expected = np.float32(np.float32(1200 * 0.37) / 32768) * np.float32(0.73)
          assert np.any(samples[:, 0] == expected), (samples[:, 0], expected)
          send('selfdriveState', enabled=False, alertSound=7)
          before = event_count('callback')
          wait_for(lambda: event_count('callback') >= before + 3)
          (params / 'SoundLanguageSetting').write_text('ko')
          wait_for(lambda: 'language changed: ko' in (output / 'stderr.log').read_text(), timeout=3)
          before = event_count('callback')
          wait_for(lambda: event_count('callback') >= before + 2)
          samples = np.fromfile(output / 'samples.f32', dtype=np.float32).reshape(-1, 4096)
          assert np.any(samples[:, 0] == np.float32(800 / 32768) * np.float32(0.73))
          send('selfdriveState', enabled=False, alertSound=0)
          send('carrotMan', leftSec=3)
          before = event_count('callback')
          wait_for(lambda: event_count('callback') >= before + 2)
          send('carrotMan', leftSec=-1)
          send('selfdriveState', enabled=False, alertSound=0)
          before = event_count('callback')
          wait_for(lambda: event_count('callback') >= before + 2)
          start_sample = event_count('callback')
          send('carState', buttonEvents=[{'type': 'mainCruise', 'pressed': True}])
          wait_for(lambda: event_count('callback') >= start_sample + 27, timeout=4)
          samples = np.fromfile(output / 'samples.f32', dtype=np.float32).reshape(-1, 4096)[start_sample:]
          assert np.all(samples[:20] == 0), 'MAIN fired before two seconds'
          assert np.any(samples[:, 0] == np.float32(800 / 32768) * np.float32(0.73)), 'MAIN did not produce prompt'
          send('carState', buttonEvents=[{'type': 'mainCruise', 'pressed': False}])
          send('selfdriveState', enabled=True, alertSound=0)
          start_sample = event_count('callback')
          wait_for(lambda: event_count('callback') >= start_sample + 65, timeout=7)
          samples = np.fromfile(output / 'samples.f32', dtype=np.float32).reshape(-1, 4096)[start_sample:]
          assert np.all(samples[2:55] == 0), 'timeout fired before five seconds'
          assert np.any(samples[61:, 0] == np.float32(1200 / 32768) * np.float32(0.73)), 'timeout warning did not play'
          process.send_signal(signal.SIGTERM)
          code = process.wait(timeout=2)
          assert code == 0
      events = lines(output / 'events.jsonl')
      assert len({row['pid'] for row in events}) == 1 == (process.pid in {row['pid'] for row in events})
      opens = [row for row in events if row['event'] == 'open']
      assert len(opens) == {'retry': 3, 'exhausted': 10}.get(mode, 1)
      if mode == 'retry':
        assert all(2.95 < b['time'] - a['time'] < 4 for a, b in zip(opens, opens[1:], strict=False))
      if mode not in ['stop-retry', 'exhausted']:
        assert [row['event'] for row in events[-3:]] == ['stop', 'close', 'terminate']
      assert events[-1]['event'] == 'terminate'
      polls = [row['time'] for row in events if row['event'] == 'poll']
      if len(polls) > 2:
        cadence = statistics.median(np.diff(polls))
        assert 0.04 < cadence < 0.065, cadence
      else:
        cadence = None
      summary = {
        'passed': True,
        'mode': mode,
        'pid': process.pid,
        'exit': code,
        'opens': len(opens),
        'callbacks': event_count('callback'),
        'median_period': cadence,
        'packets': packets,
        'audio_hardware_opened': False,
      }
      (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
      return summary
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait(timeout=2)
    publishers.clear()
    shutil.rmtree(shm)
    if old is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = old


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--mode', choices=['live', 'bounded', 'retry', 'inactive', 'start-failed', 'stop-retry', 'exhausted'])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  library = args.output.resolve() / 'libowned-portaudio.so'
  source = Path(__file__).with_name('fixtures') / 'soundd/portaudio.c'
  compile_result = subprocess.run(
    ['cc', '-std=c11', '-shared', '-fPIC', '-pthread', '-Wall', '-Wextra', '-Werror', str(source), '-o', str(library)],
    text=True,
    capture_output=True,
    check=True,
  )
  (args.output / 'compile.log').write_text(compile_result.stdout + compile_result.stderr + 'compiled owned PortAudio ABI fixture\n')
  modes = [args.mode] if args.mode else ['live', 'bounded', 'retry', 'inactive', 'start-failed', 'stop-retry', 'exhausted']
  results = [scenario(args.binary, library, args.output / mode, mode) for mode in modes]
  (args.output / 'summary.json').write_text(json.dumps(results, indent=2) + '\n')
  print(json.dumps({'passed': True, 'scenarios': len(results)}))


if __name__ == '__main__':
  main()
