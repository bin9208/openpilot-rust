#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pycapnp", "pyzmq"]
# ///
# ─── How to run ───
# PYTHONPATH=<original-msgq>:. python rust/tools/check_selfdrived_failures.py --binary <openpilot-selfdrived> --output <evidence>
# ──────────────────
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import struct
import subprocess
import tempfile
import time

from openpilot.cereal import car, log, messaging
from selfdrived_cases import event, healthy_messages, TOPICS
from selfdrived_stderr import wait_for_stderr


def run_case(binary: Path, output: Path, name: str) -> dict[str, str | int]:
  with ExitStack() as resources:
    shm = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix='msgq_selfdrive_fault_', dir='/dev/shm')))
    root = Path(resources.enter_context(tempfile.TemporaryDirectory(prefix='selfdrive-fault-')))
    prefix = shm.name.removeprefix('msgq_')
    os.environ['OPENPILOT_PREFIX'] = prefix
    namespace = root / prefix
    namespace.mkdir()
    cp = car.CarParams.new_message(brand='other', networkLocation='fwdCamera', pcmCruise=True,
                                  openpilotLongitudinalControl=True, alphaLongitudinalAvailable=True,
                                  safetyConfigs=[{'safetyModel': 'toyota'}])
    for key, raw in {'CarParams': cp.to_bytes(), 'DisableDM': b'1', 'UseWideCamera': b'0', 'LongitudinalPersonality': b'1'}.items():
      (namespace / key).write_bytes(raw)
    match name:
      case 'wait-sigterm':
        (namespace / 'CarParams').unlink()
      case 'truncated-carparams':
        (namespace / 'CarParams').write_bytes(b'bad')
      case 'fatal-integer':
        (namespace / 'DisableDM').write_bytes(b'bad')
      case 'unknown-gear' | 'wrong-union' | 'truncated-carstate' | 'unknown-personality' | 'missing-alert-text' | 'bounded-frames':
        pass
      case _:
        raise AssertionError(name)
    env = os.environ.copy()
    env.update(PARAMS_ROOT=str(root), OPENPILOT_ROOT=str(Path(__file__).resolve().parents[2]), SIMULATION='1', REPLAY='1', LOGPRINT='info')
    topics = [*TOPICS, 'driverCameraState', 'accelerometer', 'gyroscope', 'gpsLocation', 'carState']
    publishers = messaging.PubMaster(topics)
    observed = messaging.sub_sock('selfdriveState', conflate=False, timeout=100)
    command = [str(binary)] + (['--frames', '20'] if name == 'bounded-frames' else [])
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    records, ready, stderr_prefix = [], False, bytearray()
    try:
      if name == 'wait-sigterm':
        assert process.stderr is not None
        wait_for_stderr(process.stderr, b'waiting for CarParams', stderr_prefix)
        assert process.poll() is None
        process.send_signal(signal.SIGTERM)
      elif name not in ('truncated-carparams', 'fatal-integer'):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
          assert process.poll() is None, process.communicate()
          for raw in healthy_messages():
            with log.Event.from_bytes(bytes(raw)) as message:
              publishers.send(message.which(), bytes(raw))
          message = messaging.new_message('carState')
          message.carState = car.CarState.new_message(canValid=True, gearShifter='drive', vEgo=20., vCruise=100.,
                                                      cruiseState={'available': True, 'enabled': True})
          publishers.send('carState', message)
          raw = observed.receive()
          if raw is not None:
            with log.Event.from_bytes(raw) as state:
              records.append(state.to_dict())
              if state.selfdriveState.enabled:
                ready = True
                break
        assert ready, 'owned native daemon never enabled before fault'
        message.clear_write_flag()
        match name:
          case 'wrong-union':
            fault = bytes(event('userBookmark'))
          case 'truncated-carstate':
            fault = b'bad'
          case 'unknown-gear':
            message.carState.gearShifter = 65535
            fault = message.to_bytes()
          case 'unknown-personality':
            (namespace / 'LongitudinalPersonality').write_bytes(b'10')
            fault = message.to_bytes()
          case 'missing-alert-text':
            publishers.send('longitudinalPlan', bytes(event('longitudinalPlan', events=[{'name': 'torqueNNLoad'}])))
            fault = message.to_bytes()
          case 'bounded-frames':
            fault = message.to_bytes()
          case _:
            raise AssertionError(name)
        deadline = time.monotonic() + 5
        while process.poll() is None and time.monotonic() < deadline:
          publishers.send('carState', fault)
          raw = observed.receive()
          if raw is not None:
            with log.Event.from_bytes(raw) as state:
              records.append(state.to_dict())
      stdout, stderr = process.communicate(timeout=5)
      stdout = stdout.decode()
      stderr = (stderr_prefix + stderr).decode()
      while (raw := observed.receive(non_blocking=True)) is not None:
        with log.Event.from_bytes(raw) as state:
          records.append(state.to_dict())
      (output / f'{name}.stdout').write_text(stdout or 'no stdout\n')
      (output / f'{name}.stderr').write_text(stderr or 'no stderr\n')
      (output / f'{name}.messages.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in records) or 'no publication before startup rejection\n')
      match name:
        case 'wait-sigterm' | 'bounded-frames':
          assert process.returncode == 0, (name, process.returncode, stderr)
          if name == 'bounded-frames':
            with (shm / 'selfdriveState').open('rb') as queue:
              header = queue.read(struct.calcsize('<123Q'))
              write_pointer = struct.unpack('<123Q', header)[1]
              assert write_pointer >> 32 == 0, 'bounded output queue unexpectedly wrapped'
              payload = queue.read(write_pointer & 0xffffffff)
            (output / 'bounded-frames.queue.bin').write_bytes(header + payload)
            position, publications = 0, []
            while position < len(payload):
              size = struct.unpack_from('<q', payload, position)[0]
              assert size > 0
              with log.Event.from_bytes(payload[position+8:position+8+size]) as state:
                assert state.which() == 'selfdriveState' and state.valid
                publications.append(state.to_dict())
              position += (8 + size + 7) & -8
            assert position == len(payload) and len(publications) == 20, len(publications)
            (output / 'bounded-frames.queued-messages.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in publications))
        case 'fatal-integer':
          assert process.returncode == -signal.SIGABRT, (name, process.returncode, stderr)
        case 'wrong-union':
          assert process.returncode == 1 and 'carState event union' in stderr, (name, process.returncode, stderr)
        case 'unknown-gear':
          assert process.returncode == 1 and 'enum' in stderr.lower(), (name, process.returncode, stderr)
        case 'unknown-personality':
          assert process.returncode == 1 and 'personality has no source enum name: 10' in stderr, (name, process.returncode, stderr)
        case 'missing-alert-text':
          assert process.returncode == 1 and 'null alertText2 rejected at cereal encoding' in stderr, (name, process.returncode, stderr)
        case 'truncated-carparams' | 'truncated-carstate':
          assert process.returncode == 1 and 'selfdrived:' in stderr, (name, process.returncode, stderr)
        case _:
          raise AssertionError(name)
      return {'scenario': name, 'exit': process.returncode, 'published_messages': len(records), 'result': 'PASS'}
    finally:
      if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=5)
        (output / f'{name}.stdout').write_text(stdout.decode() or 'no stdout\n')
        (output / f'{name}.stderr').write_text((stderr_prefix + stderr).decode() or 'no stderr\n')


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
  args.output.mkdir(parents=True, exist_ok=False)
  names = ('wait-sigterm', 'truncated-carparams', 'fatal-integer', 'wrong-union', 'truncated-carstate',
           'unknown-gear', 'unknown-personality', 'missing-alert-text', 'bounded-frames')
  results = [run_case(args.binary.resolve(), args.output, name) for name in names]
  report = {'result': 'PASS', 'scenarios': results, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
