#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp", "pyzmq"]
# ///
# ─── How to run ───
# PYTHONPATH=<original-msgq>:. python rust/tools/check_selfdrived_ipc.py --binary <openpilot-selfdrived> --output <evidence>
# ──────────────────
"""Owned host peers exercise actual SubMaster, nonconflated carState and graceful restart."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import tempfile
import time

from openpilot.cereal import car, log, messaging
from selfdrived_cases import TOPICS, event


def receive(socket, timeout=100):
  raw = socket.receive(non_blocking=timeout == 0)
  if raw is None:
    return None
  with log.Event.from_bytes(raw) as message:
    return message.to_dict()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--elf', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  with ExitStack() as resources:
    shm = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix='msgq_selfdrive_ipc_', dir='/dev/shm')))
    prefix = shm.name.removeprefix('msgq_')
    os.environ['OPENPILOT_PREFIX'] = prefix
    topics = [*TOPICS, 'driverCameraState', 'accelerometer', 'gyroscope', 'gpsLocation', 'carState']
    publishers = messaging.PubMaster(topics)
    output = messaging.sub_sock('selfdriveState', conflate=False, timeout=100)
    onroad = messaging.sub_sock('onroadEvents', conflate=False, timeout=100)
    records = []
    onroad_records = []
    identities = []
    with tempfile.TemporaryDirectory(prefix='selfdrive-ipc-') as tmp:
      root = Path(tmp)
      namespace = root / prefix
      namespace.mkdir()
      cp = car.CarParams.new_message(brand='other', networkLocation='fwdCamera', pcmCruise=True,
                                    openpilotLongitudinalControl=True, alphaLongitudinalAvailable=True,
                                    safetyConfigs=[{'safetyModel': 'toyota'}])
      for key, raw in {'CarParams': cp.to_bytes(), 'DisableDM': b'1', 'UseWideCamera': b'0', 'LongitudinalPersonality': b'1',
                       'IsMetric': b'1', 'ExperimentalMode': b'1', 'AlphaLongitudinalEnabled': b'1'}.items():
        (namespace / key).write_bytes(raw)
      env = os.environ.copy()
      env.update(PARAMS_ROOT=str(root), OPENPILOT_ROOT=str(Path(__file__).resolve().parents[2]), SIMULATION='1', REPLAY='1', LOGPRINT='info')
      values = {'pandaStates': {'safetyModel': 'toyota', 'controlsAllowed': True},
                'liveCalibration': {'calStatus': 'calibrated', 'rpyCalib': [0., 0., 0.]},
                'liveParameters': {'valid': True}, 'livePose': {'inputsOK': True, 'posenetOK': True},
                'deviceState': {'freeSpacePercent': 50.}, 'modelV2': {'velocity': {'x': [20.]}}}
      inputs = {topic: bytes(event(topic, **values.get(topic, {}))) for topic in topics
                if topic not in ('carState', 'alertDebug', 'userBookmark', 'audioFeedback')}
      def send_car(speed):
        message = messaging.new_message('carState')
        message.valid = True
        message.carState = car.CarState.new_message(canValid=True, gearShifter='drive', cruiseState={'available': True, 'enabled': True},
                                                    vEgo=speed, vCruise=100.)
        publishers.send('carState', message)
      for run in range(2):
        (namespace / 'ExperimentalMode').write_bytes(b'1')
        (namespace / 'LongitudinalPersonality').write_bytes(b'1')
        if run:
          # Original queue close retains reader slots; restart owned publishers too.
          publishers = messaging.PubMaster(topics)
          while receive(output, 0) is not None:
            pass
          while receive(onroad, 0) is not None:
            pass
        process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
          started = time.monotonic()
          ready = None
          while time.monotonic()-started < 10:
            assert process.poll() is None, process.communicate()
            for topic, raw in inputs.items():
              publishers.send(topic, raw)
            send_car(20.)
            ready = receive(output)
            if ready is not None:
              records.append(ready)
              state = ready['selfdriveState']
              if state['enabled'] and state['active'] and state['engageable'] and state['experimentalMode'] and state['personality'] == 'standard':
                break
          assert ready is not None and ready['selfdriveState']['enabled'], 'startup never enabled'
          identity = {'exe': os.readlink(f'/proc/{process.pid}/exe'),
                      'maps': Path(f'/proc/{process.pid}/maps').read_text(),
                      'cmdline': Path(f'/proc/{process.pid}/cmdline').read_bytes().split(b'\0')[:-1]}
          identity['cmdline'] = [value.decode() for value in identity['cmdline']]
          if args.elf is None:
            assert identity['exe'] == str(args.binary.resolve())
          else:
            assert str(args.elf.resolve()) in identity['cmdline']
            assert str(args.elf.resolve()) in identity['maps']
          identity['launcher_sha256'] = hashlib.sha256(Path(identity['exe']).read_bytes()).hexdigest()
          assert 'libpython' not in identity['maps']
          identities.append(identity)
          events = receive(onroad)
          assert events is not None and events['valid'] and isinstance(events['onroadEvents'], list)
          onroad_records.append(events)
          # Wait until warm-up inputs have actually reached every native reader.
          started = time.monotonic()
          while not all(publishers.sock[topic].all_readers_updated() for topic in inputs.keys() | {'carState'}):
            if time.monotonic()-started >= 5:
              headers = {}
              for topic in inputs.keys() | {'carState'}:
                with (shm / topic).open('rb') as stream:
                  header = list(struct.unpack('<123Q', stream.read(984)))
                headers[topic] = {'readers': header[0], 'write': header[1], 'read': header[3:43], 'valid': header[43:83]}
              (args.output / 'ack-headers.json').write_text(json.dumps(headers, indent=2)+'\n')
              raise AssertionError(('warm-up readers did not acknowledge', headers['carState']))
            time.sleep(.001)
          # Freeze only the owned daemon while filling the real nonconflated queue.
          process.send_signal(signal.SIGSTOP)
          while receive(output, 0) is not None:
            pass
          while receive(onroad, 0) is not None:
            pass
          for speed in range(11, 21):
            send_car(float(speed))
          process.send_signal(signal.SIGCONT)
          burst = []
          started = time.monotonic()
          while len(burst) < 20 and time.monotonic()-started < 5:
            message = receive(output)
            if message is not None:
              burst.append(message)
              records.append(message)
          distances = [message['selfdriveState']['distanceTraveled'] for message in burst]
          deltas = [right-left for left, right in zip(distances[:-1], distances[1:], strict=True)]
          target = [speed * .01 for speed in range(12, 21)]
          match = any(all(abs(actual-expected) < 1e-4 for actual, expected in zip(deltas[start:start+9], target, strict=True))
                      for start in range(len(deltas)-8))
          burst_report = {'distances': distances, 'deltas': deltas, 'source_step_increments': target}
          (args.output / f'run-{run}.burst.json').write_text(json.dumps(burst_report, indent=2)+'\n')
          assert match, ('nonconflated burst lost an intermediate carState', distances, deltas)
          (namespace / 'ExperimentalMode').write_bytes(b'0')
          (namespace / 'LongitudinalPersonality').write_bytes(b'2')
          started = time.monotonic()
          bookmarked = False
          while time.monotonic() - started < 5:
            for topic, raw in inputs.items():
              publishers.send(topic, raw)
            send_car(20.)
            publishers.send('userBookmark', bytes(event('userBookmark')))
            events = receive(onroad, 0)
            if events is not None:
              assert events['valid']
              onroad_records.append(events)
              bookmarked |= any(item['name'] == 'userBookmark' for item in events['onroadEvents'])
            updated = receive(output)
            if updated is not None:
              records.append(updated)
              value = updated['selfdriveState']
              if not value['experimentalMode'] and value['personality'] == 'relaxed' and bookmarked:
                break
          else:
            raise AssertionError('native Params worker did not publish refreshed settings')
          process.send_signal(signal.SIGINT)
          stdout, stderr = process.communicate(timeout=5)
          assert process.returncode == 0, (process.returncode, stderr)
          (args.output / f'run-{run}.stdout').write_text(stdout or 'no stdout\n')
          (args.output / f'run-{run}.stderr').write_text(stderr)
        finally:
          if process.poll() is None:
            process.send_signal(signal.SIGCONT)
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=5)
            (args.output / f'run-{run}.stdout').write_text(stdout or 'no stdout\n')
            (args.output / f'run-{run}.stderr').write_text(stderr or 'no stderr\n')
          (args.output / 'messages.jsonl').write_text(''.join(json.dumps(record)+'\n' for record in records))
      (args.output / 'messages.jsonl').write_text(''.join(json.dumps(record)+'\n' for record in records))
      (args.output / 'onroad-messages.jsonl').write_text(''.join(json.dumps(record)+'\n' for record in onroad_records))
      (args.output / 'native-processes.json').write_text(json.dumps(identities, indent=2)+'\n')
      report = {'result': 'PASS', 'startup_restart_runs': 2, 'observed_messages': len(records),
                'nonconflated_bursts': 2, 'params_refresh_runs': 2,
                'onroad_messages': len(onroad_records),
                'binary_sha256': hashlib.sha256((args.elf or args.binary).read_bytes()).hexdigest(),
                'command_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                'shutdown': 'SIGINT joins Params worker; second start uses same owned namespace'}
      (args.output / 'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
      print(json.dumps(report))



if __name__ == '__main__':
  main()
