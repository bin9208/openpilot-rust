#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import tempfile
import time
import uuid

from openpilot.cereal import log
from check_locationd_daemon import read_line, readers_ready
from check_paramsd import compare
from controlsd_fixture import SETTINGS, frame, parameters
from controlsd_scenarios import change
from controlsd_source import trace


def writers_ready(shm, pid):
  for name in ('controlsState', 'carControl'):
    with (shm / name).open('rb') as stream:
      uid = struct.unpack('<3Q', stream.read(24))[2]
    if uid & 0xFFFFFFFF != pid:
      return False
  return True


def phase(target, numerics, assets, output, signum, saved=None):
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'rust-probe-controls-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  daemon, peer = None, None
  packets, actual = [], []
  with tempfile.TemporaryDirectory(prefix='controlsd-') as temporary:
    root = Path(temporary)
    params = root / 'params' / prefix
    params.mkdir(parents=True)
    settings = SETTINGS | {'NNFF': '1'}
    cp = parameters('HYUNDAI_PALISADE', 'torque', settings)
    if saved:
      settings.update(saved)
    for key, value in settings.items():
      (params / key).write_text(value)
    environment = os.environ | {
      'OPENPILOT_PREFIX': prefix,
      'PARAMS_ROOT': str(root / 'params'),
      'SIMULATION': '1',
      'PATH': str(root / 'no-programs'),
      'OPENBLAS_NUM_THREADS': '1',
    }
    for key in ('ZMQ', 'CEREAL_FAKE'):
      environment.pop(key, None)
    try:
      with (output / 'stdout.log').open('w') as stdout, (output / 'stderr.log').open('w') as stderr, (output / 'peer.log').open('w') as peer_log:
        peer = subprocess.Popen(
          [target / 'debug/examples/controlsd_peer'], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_log, text=True
        )
        assert read_line(peer) == {'ready': True}
        daemon = subprocess.Popen(
          [target / 'debug/openpilot-controlsd', '--numerics', numerics, '--assets', assets], env=environment, stdout=stdout, stderr=stderr
        )
        time.sleep(0.15)
        assert daemon.poll() is None
        assert (params / 'LongitudinalPersonalityMax').exists() == bool(saved)
        assert not any(readers_ready(shm, ['selfdriveState'], daemon.pid).values())
        (params / 'CarParams').write_bytes(bytes(cp))
        names = [
          'selfdriveState',
          'carState',
          'liveParameters',
          'modelV2',
          'liveCalibration',
          'livePose',
          'longitudinalPlan',
          'carOutput',
          'carrotMan',
          'lateralPlan',
          'radarState',
          'driverMonitoringState',
          'onroadEvents',
          'driverAssistance',
          'liveDelay',
          'liveTorqueParameters',
        ]
        deadline = time.monotonic() + 10
        while not (all(readers_ready(shm, names, daemon.pid).values()) and writers_ready(shm, daemon.pid)):
          assert daemon.poll() is None, (output / 'stderr.log').read_text()
          assert time.monotonic() < deadline
          time.sleep(0.0005)
        row = change(frame(30), omit=['carrotMan'])
        count = 140
        peer.stdin.write(json.dumps({'packets': row['messages'], 'count': count}) + '\n')
        peer.stdin.flush()
        executable = os.readlink(f'/proc/{daemon.pid}/exe')
        assert executable == str(target / 'debug/openpilot-controlsd')
        mappings = Path(f'/proc/{daemon.pid}/maps').read_text()
        (output / 'maps.txt').write_text(mappings)
        assert all(name not in mappings for name in ('libpython', 'numpy/_core', 'cython'))
        assert 'libscipy_openblas' in mappings
        commandline = Path(f'/proc/{daemon.pid}/cmdline').read_bytes()
        (output / 'cmdline.bin').write_bytes(commandline)
        source_frames = []
        received = False
        for _ in range(count):
          pair = read_line(peer)['packets']
          packets.append(pair)
          values = []
          for packet in pair:
            with log.Event.from_bytes(bytes(packet)) as event:
              values.append({'service': event.which(), 'event': event.to_dict()})
          assert [value['service'] for value in values] == ['controlsState', 'carControl']
          assert values[0]['event']['logMonoTime'] <= values[1]['event']['logMonoTime']
          has_input = values[0]['event']['controlsState']['lateralPlanMonoTime'] != 0
          source_frames.append({'time': values[0]['event']['logMonoTime'] / 1e9, 'messages': row['messages'] if has_input and not received else []})
          received |= has_input
          actual.append(values)
        assert received
        assert (params / 'LongitudinalPersonalityMax').read_text() == '3'
        assert (params / 'HyundaiCameraSccHint').read_text() == '0'
        before = time.monotonic()
        daemon.send_signal(signum)
        assert daemon.wait(timeout=5) == 0
        shutdown = time.monotonic() - before
        peer.stdin.close()
        assert peer.wait(timeout=5) == 0
        case = {
          'name': 'native-signal-' + str(signum),
          'params': {key: list(value.encode()) for key, value in settings.items()} | {'CarParams': cp},
          'simulation': True,
          'frames': source_frames,
        }
        expected = trace(case)
        (output / 'input.json').write_text(json.dumps(case) + '\n')
        (output / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
        errors = []
        for reference, values in zip(expected['rows'], actual, strict=True):
          for left, right in zip(reference['publications'], values, strict=True):
            left = left.copy()
            left['event'] = left['event'].copy()
            left['event'].pop('logMonoTime')
            right = right.copy()
            right['event'] = right['event'].copy()
            right['event'].pop('logMonoTime')
            compare(left, right, 'native.event', errors)
        persisted = {key: (params / key).read_text() for key in ('LongitudinalPersonalityMax', 'HyundaiCameraSccHint')}
        (output / 'persisted.json').write_text(json.dumps(persisted, indent=2) + '\n')
        result = {
          'pass': True,
          'restart_from_persisted_params': bool(saved),
          'frames': count,
          'publications': count * 2,
          'source_equal': True,
          'max_error': max(errors),
          'startup_waits_for_carparams': True,
          'parameter_writes': {'LongitudinalPersonalityMax': '3', 'HyundaiCameraSccHint': '0'},
          'signal': int(signum),
          'exit': daemon.returncode,
          'shutdown_seconds': shutdown,
          'native_executable': executable,
          'no_python_runtime': True,
        }
        (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result))
        return persisted
    finally:
      for process in (peer, daemon):
        if process is not None and process.poll() is None:
          process.kill()
          process.wait(timeout=5)
      (output / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
      (output / 'packets.json').write_text(json.dumps(packets) + '\n')
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--assets', type=Path, default=Path('opendbc_repo/opendbc/car/torque_data'))
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  saved = None
  for signum in (signal.SIGINT, signal.SIGTERM):
    saved = phase(args.target.resolve(), args.numerics.resolve(), args.assets.resolve(), args.evidence / str(signum), signum, saved)


if __name__ == '__main__':
  main()
