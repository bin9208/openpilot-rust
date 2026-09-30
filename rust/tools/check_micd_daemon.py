#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import uuid

import msgq
import numpy as np
from openpilot.cereal import log
from check_micd_analysis import source


def events(path):
  if not path.exists():
    return []
  return [json.loads(line) for line in path.read_text().splitlines() if line.endswith('}')]


def scenario(binary, library, root, mode):
  root.mkdir(parents=True)
  prefix = 'mic-' + uuid.uuid4().hex[:20]
  old_prefix = os.environ.get('OPENPILOT_PREFIX')
  os.environ['OPENPILOT_PREFIX'] = prefix
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  environment = os.environ | {'MIC_FIXTURE_EVENTS': str(root / 'events.jsonl'), 'MIC_FIXTURE_GO': str(root / 'go'), 'LOGPRINT': 'info'}
  if mode in ('retry', 'stop-retry'):
    environment['MIC_FIXTURE_FAIL_OPEN'] = '10' if mode == 'stop-retry' else '1'
  if mode == 'start-failed':
    environment['MIC_FIXTURE_FAIL_START'] = '1'
  process = None
  subscribers = {}
  try:
    with (root / 'stdout.log').open('w') as stdout, (root / 'stderr.log').open('w') as stderr:
      process = subprocess.Popen([str(binary), '--portaudio', str(library)], env=environment, stdout=stdout, stderr=stderr)
      def wait_for(predicate):
        deadline = time.monotonic() + 8
        while not predicate():
          assert process.poll() is None, (root / 'stderr.log').read_text()
          assert time.monotonic() < deadline, (mode, events(root / 'events.jsonl'))
          time.sleep(.005)
      if mode == 'start-failed':
        assert process.wait(timeout=5) == 1
        assert 'PortAudio start' in (root / 'stderr.log').read_text()
      elif mode == 'stop-retry':
        wait_for(lambda: any(row['event'] == 'open' for row in events(root / 'events.jsonl')))
        process.send_signal(signal.SIGTERM)
        assert process.wait(timeout=2) == 0
      else:
        wait_for(lambda: any(row['event'] == 'ready' for row in events(root / 'events.jsonl')))
        subscribers = {topic: msgq.sub_sock(topic, timeout=100) for topic in ('rawAudioData', 'soundPressure')}
        for subscriber in subscribers.values():
          subscriber.receive(non_blocking=True)
        original, _ = source()
        raw = []
        possible = [(0., 0., 0.)]
        for block in range(16):
          samples = ((np.arange(800) + block) % 17 - 8).astype(np.float32) / 32
          original.callback(samples.reshape(-1, 1), 800, None, None)
          raw.append(original.pm.raw)
          possible.append((original.sound_pressure, original.sound_pressure_weighted, original.sound_pressure_level_weighted))
        (root / 'go').touch()
        packets = {topic: [] for topic in subscribers}
        pressure_times = []
        observed = []
        started = time.monotonic()
        while time.monotonic() - started < 1.5:
          for topic, subscriber in subscribers.items():
            while (packet := subscriber.receive(non_blocking=True)) is not None:
              packets[topic].append(packet)
              with log.Event.from_bytes(packet) as event:
                assert event.valid and event.which() == topic
                assert 0 < event.logMonoTime <= time.monotonic_ns()
                if topic == 'rawAudioData':
                  assert event.rawAudioData.sampleRate == 16000
                  observed.append(bytes(event.rawAudioData.data))
                else:
                  pressure_times.append(event.logMonoTime)
                  value = event.soundPressure
                  values = (value.soundPressure, value.soundPressureWeighted, value.soundPressureWeightedDb)
                  assert any(np.allclose(values, candidate, rtol=1e-6, atol=1e-9) for candidate in possible), values
          assert process.poll() is None
          time.sleep(.003)
        assert observed == raw, (len(observed), len(raw))
        assert 12 <= len(pressure_times) <= 18, len(pressure_times)
        cadence = (pressure_times[-1] - pressure_times[0]) / (len(pressure_times) - 1) / 1e9
        assert .08 <= cadence <= .12, cadence
        assert np.allclose(values, possible[-1], rtol=1e-6, atol=1e-9), values
        for topic, values in packets.items():
          (root / (topic + '.capnp')).write_bytes(b''.join(values))
        process.send_signal(signal.SIGTERM)
        assert process.wait(timeout=2) == 0
        assert 'micd stream started' in (root / 'stdout.log').read_text() + (root / 'stderr.log').read_text()
        report = {'raw_packets': len(observed), 'pressure_packets': len(pressure_times), 'mean_interval_seconds': cadence}
        (root / 'ipc.json').write_text(json.dumps(report, indent=2))
      recorded = events(root / 'events.jsonl')
      names = [row['event'] for row in recorded]
      assert names[-1] == 'terminate', names
      if mode != 'stop-retry':
        assert names[-3:] == ['stop', 'close', 'terminate'], names
      if mode == 'retry':
        opens = [row for row in recorded if row['event'] == 'open']
        assert len(opens) == 2 and opens[1]['ns'] - opens[0]['ns'] >= 2_900_000_000, opens
      assert 'poll' not in names
      print('PASS:', mode)
  finally:
    if process is not None and process.poll() is None:
      process.kill()
      process.wait(timeout=5)
    subscribers.clear()
    shutil.rmtree(shm)
    if old_prefix is None:
      os.environ.pop('OPENPILOT_PREFIX', None)
    else:
      os.environ['OPENPILOT_PREFIX'] = old_prefix


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--cc', default='cc')
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=True)
  library = args.output / 'libmic-fixture.so'
  subprocess.run([args.cc, '-std=c11', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', '-pthread',
                  str(Path(__file__).with_name('fixtures') / 'micd/portaudio.c'), '-o', str(library)], check=True)
  for mode in ('live', 'retry', 'stop-retry', 'start-failed'):
    scenario(args.binary.resolve(), library, args.output / mode, mode)


if __name__ == '__main__':
  main()
