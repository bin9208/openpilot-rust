#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import select
import shutil
import signal
import struct
import subprocess
import tempfile
import time
import uuid

from openpilot.cereal import log
from check_locationd import compare
from locationd_loop_fixture import cases
from locationd_loop_source import trace


def read_line(process):
  assert select.select([process.stdout], [], [], 15)[0], 'peer timeout'
  return json.loads(process.stdout.readline())


def readers_ready(shm, names, pid):
  ready = {}
  for name in names:
    with (shm / name).open('rb') as stream:
      header = struct.unpack('<123Q', stream.read(984))
    ready[name] = any(header[43 + i] and header[83 + i] & 0xFFFFFFFF == pid for i in range(min(header[0], 40)))
  return ready


def run(target, oracle, case, output, signum):
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'rust-probe-location-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  daemon, peer = None, None
  with tempfile.TemporaryDirectory(prefix='locationd-') as temporary:
    root = Path(temporary)
    params = root / 'params' / prefix
    params.mkdir(parents=True)
    if case['seed'] is not None:
      (params / 'LocationFilterInitialState').write_bytes(bytes(case['seed']))
    environment = os.environ | {
      'OPENPILOT_PREFIX': prefix,
      'PARAMS_ROOT': str(root / 'params'),
      'DEBUG': '1',
      'SIMULATION': '1',
      'PATH': str(root / 'no-programs'),
    }
    for name in ('ZMQ', 'CEREAL_FAKE'):
      environment.pop(name, None)
    expected = trace(oracle, case)
    actual, packets, frames, latencies = [], [], [], []
    try:
      with (output / 'daemon.stdout').open('w') as stdout, (output / 'daemon.stderr').open('w') as stderr, (output / 'peer.stderr').open('w') as peer_error:
        daemon = subprocess.Popen([target / 'debug/openpilot-locationd'], env=environment, stdout=stdout, stderr=stderr)
        deadline = time.monotonic() + 10
        names = ('carState', 'liveCalibration', 'cameraOdometry', 'accelerometer', 'gyroscope', 'livePose')
        while not all((shm / name).exists() for name in names):
          assert daemon.poll() is None, (output / 'daemon.stderr').read_text()
          assert time.monotonic() < deadline, 'native subscriptions did not open'
          time.sleep(0.01)
        executable = os.readlink(f'/proc/{daemon.pid}/exe')
        assert executable == str(target / 'debug/openpilot-locationd')
        mappings = Path(f'/proc/{daemon.pid}/maps').read_text()
        (output / 'daemon.maps').write_text(mappings)
        assert 'libpython' not in mappings and 'ekf_sym_pyx' not in mappings and 'libpose.so' not in mappings
        peer = subprocess.Popen(
          [target / 'debug/examples/location_peer'], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_error, text=True
        )
        assert read_line(peer) == {'ready': True}
        deadline = time.monotonic() + 10
        while not all(readers_ready(shm, names[:-1], daemon.pid).values()):
          assert daemon.poll() is None
          assert time.monotonic() < deadline, 'msgq readers not registered'
          time.sleep(0.01)
        (output / 'reader-readiness.json').write_text(json.dumps(readers_ready(shm, names[:-1], daemon.pid), indent=2) + '\n')
        for frame, reference in zip(case['frames'], expected['rows'], strict=True):
          assert len(reference['publications']) == 1
          sent = frame['acceleration'] + frame['gyroscope'] + frame['messages']
          frames.append({'packets': sent})
          before = time.monotonic_ns()
          peer.stdin.write(json.dumps(frames[-1]) + '\n')
          peer.stdin.flush()
          received = read_line(peer)['packet']
          after = time.monotonic_ns()
          with log.Event.from_bytes(bytes(received)) as event:
            assert before <= event.logMonoTime <= after
            value = event.to_dict()
          value.pop('logMonoTime')
          expected_event = reference['publications'][0].copy()
          expected_event.pop('logMonoTime')
          compare(expected_event, value, 'daemon.event', [])
          actual.append(value)
          packets.append(received)
          latencies.append((after - before) / 1e9)
        stopped = time.monotonic()
        daemon.send_signal(signum)
        assert daemon.wait(timeout=5) == 0
        stop_seconds = time.monotonic() - stopped
        peer.stdin.close()
        assert peer.wait(timeout=5) == 0
        result = {
          'pass': True,
          'native_executable': executable,
          'publications': len(actual),
          'source_equal': True,
          'signal': int(signum),
          'exit': daemon.returncode,
          'shutdown_seconds': stop_seconds,
          'max_exchange_seconds': max(latencies),
          'no_python_or_generated_cpp_model_loaded': True,
        }
        (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result))
    finally:
      for process in (peer, daemon):
        if process is not None and process.poll() is None:
          process.kill()
          process.wait(timeout=5)
      (output / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
      (output / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
      (output / 'packets.json').write_text(json.dumps(packets) + '\n')
      (output / 'input.json').write_text(json.dumps(frames) + '\n')
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  selected = cases()
  normal = selected[1]
  normal['frames'] = normal['frames'][:115]
  run(args.target.resolve(), args.oracle.resolve(), normal, args.evidence / 'continuous', signal.SIGINT)
  run(args.target.resolve(), args.oracle.resolve(), selected[2], args.evidence / 'seed', signal.SIGTERM)


if __name__ == '__main__':
  main()
