#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import uuid

from openpilot.cereal import log
from check_locationd_daemon import read_line, readers_ready
from check_paramsd import compare
from paramsd_fixture import car_params, event, pose
from paramsd_loop_source import trace
from paramsd_startup_gate import StartupGate, build_library


def frames(gps, count):
  result = []
  for i in range(count):
    timestamp = 100. + i * .05
    messages = [event('carState', timestamp, {'vEgo': 18., 'steeringAngleDeg': 2.}),
                event('liveCalibration', timestamp, {'rpyCalib': [.01, .02, -.01], 'calStatus': 'calibrated'}),
                event(gps, timestamp, {'hasFix': True, 'latitude': 37.5, 'longitude': 127., 'bearingDeg': 22.5}, valid=False),
                pose(timestamp)]
    result.append({'time': timestamp, 'messages': messages})
  return result


def phase(target, oracle, paths, restart):
  root, output = paths
  output.mkdir(parents=True, exist_ok=True)
  library = build_library(root, output)
  gps = 'gpsLocation' if restart else 'gpsLocationExternal'
  prefix = 'rust-probe-params-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  params = root / 'params' / prefix
  memory = root / 'memory' / prefix
  params.mkdir(parents=True)
  memory.mkdir(parents=True)
  (memory / 'LastGPSPosition').write_text('old position')
  (params / 'UbloxAvailable').write_bytes(b'0' if restart else b'1')
  seed = {}
  if restart:
    seed = {'LiveParametersV2': list((root / 'saved.bin').read_bytes()), 'CarParamsPrevRoute': car_params()}
    for key, data in seed.items():
      (params / key).write_bytes(bytes(data))
  case = {'name': 'restart' if restart else 'continuous', 'car': car_params(), 'seed': seed, 'debug': True,
          'replay': True, 'simulation': True, 'gps': gps, 'frames': frames(gps, 8 if restart else 1202)}
  case['frames'].insert(0, {'time': 99.95, 'messages': []})
  expected = trace(oracle, case)
  gate = StartupGate(memory / 'LastGPSPosition', shm / 'livePose', output)
  environment = os.environ | {'OPENPILOT_PREFIX': prefix, 'PARAMS_ROOT': str(root / 'params'), 'DEBUG': '1',
                             'REPLAY': '1', 'SIMULATION': '1', 'PATH': str(root / 'no-programs')}
  for name in ('ZMQ', 'CEREAL_FAKE'):
    environment.pop(name, None)
  daemon, peer = None, None
  actual, packets = [], []
  try:
    with (output / 'stdout.log').open('w') as stdout, (output / 'stderr.log').open('w') as stderr, (output / 'peer.log').open('w') as peer_log:
      daemon = subprocess.Popen([target / 'debug/openpilot-paramsd', '--memory-root', root / 'memory'],
        env=environment | gate.environment(library), stdout=stdout, stderr=stderr, pass_fds=gate.inherited)
      gate.spawned()
      names = ('carState', 'liveCalibration', gps, 'livePose')
      deadline = time.monotonic() + 10
      while not all((shm / name).exists() for name in (*names, 'liveParameters')):
        assert daemon.poll() is None, (output / 'stderr.log').read_text()
        assert time.monotonic() < deadline
        time.sleep(.01)
      assert (memory / 'LastGPSPosition').read_text() == 'old position'
      executable = os.readlink(f'/proc/{daemon.pid}/exe')
      assert executable == str(target / 'debug/openpilot-paramsd')
      mappings = Path(f'/proc/{daemon.pid}/maps').read_text()
      (output / 'maps.txt').write_text(mappings)
      assert all(name not in mappings for name in ('libpython', 'ekf_sym_pyx', 'libcar.so'))
      peer = subprocess.Popen([target / 'debug/examples/paramsd_peer', gps], env=environment, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, stderr=peer_log, text=True)
      assert read_line(peer) == {'ready': True}
      (params / 'CarParams').write_bytes(bytes(case['car']))
      deadline = time.monotonic() + 10
      while not all(readers_ready(shm, names, daemon.pid).values()):
        assert daemon.poll() is None, (output / 'stderr.log').read_text()
        assert time.monotonic() < deadline
        time.sleep(.001)
      gate.wait()
      previous = gate.pointer()
      for index, frame in enumerate(case['frames'][1:]):
        before = time.monotonic_ns()
        peer.stdin.write(json.dumps({'packets': frame['messages']}) + '\n')
        peer.stdin.flush()
        if index == 0:
          gate.release(previous)
        packet = read_line(peer)['packet']
        after = time.monotonic_ns()
        with log.Event.from_bytes(bytes(packet)) as message:
          assert before <= message.logMonoTime <= after
          value = message.to_dict()
        value.pop('logMonoTime')
        actual.append(value)
        packets.append(packet)
      signum = signal.SIGTERM if restart else signal.SIGINT
      before = time.monotonic()
      daemon.send_signal(signum)
      assert daemon.wait(timeout=5) == 0
      stop_time = time.monotonic() - before
      peer.stdin.close()
      assert peer.wait(timeout=5) == 0
      saved = (params / 'LiveParametersV2').read_bytes()
      (output / 'saved.bin').write_bytes(saved)
      assert list(saved) == seed['LiveParametersV2'] if restart else list(saved) in packets
      for reference, value in zip(expected['rows'][1:], actual, strict=True):
        source = reference['event'].copy()
        source.pop('logMonoTime')
        compare(source, value, 'daemon.event', [])
      if not restart:
        assert list(saved) == packets[1199], ('1200-frame persistence after startup poll', packets.index(list(saved)))
        (root / 'saved.bin').write_bytes(saved)
      position = (memory / 'LastGPSPosition').read_bytes()
      assert position == b'{"latitude": 37.5, "longitude": 127.0, "bearing": 22.5}'
      (output / 'gps.json').write_bytes(position)
      result = {'pass': True, 'publications': len(actual), 'native_executable': executable, 'source_equal': True,
                'restart': restart, 'persisted_packet_index': None if restart else packets.index(list(saved)), 'gps_service': gps,
                'signal': int(signum), 'exit': daemon.returncode, 'shutdown_seconds': stop_time, 'no_python_runtime': True}
      result['startup_poll'] = gate.report
      (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
      print(json.dumps(result))
  finally:
    try:
      for process in (peer, daemon):
        if process is not None and process.poll() is None:
          process.kill()
          process.wait(timeout=5)
      if peer is not None:
        peer.stdin.close()
        peer.stdout.close()
      (output / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
      (output / 'native.json').write_text(json.dumps(actual, indent=2) + '\n')
      (output / 'packets.json').write_text(json.dumps(packets) + '\n')
      (output / 'input.json').write_text(json.dumps(case) + '\n')
    finally:
      gate.close()
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  with tempfile.TemporaryDirectory(prefix='paramsd-') as temporary:
    for restart in (False, True):
      phase(args.target.resolve(), args.oracle.resolve(), (Path(temporary), args.evidence / ('restart' if restart else 'continuous')), restart)


if __name__ == '__main__':
  main()
