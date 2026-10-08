from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import select
import shutil
import struct
import subprocess
import tempfile
import time
import uuid

from check_locationd_daemon import read_line, readers_ready
from check_paramsd_daemon import car_params, frames


def pointer(path: Path) -> int:
  with path.open('rb') as stream:
    return struct.unpack('<2Q', stream.read(16))[1]


def source_control(args) -> None:
  output = args.evidence
  output.mkdir(parents=True, exist_ok=False)
  prefix = 'rust-probe-params-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  root = output / 'owned'
  params, memory = (root / name / prefix for name in ('params', 'memory'))
  params.mkdir(parents=True)
  memory.mkdir(parents=True)
  (params / 'UbloxAvailable').write_bytes(b'1')
  (memory / 'LastGPSPosition').write_text('old position')
  ready_read, ready_write = os.pipe()
  release_read, release_write = os.pipe()
  env = {**os.environ, 'OPENPILOT_PREFIX': prefix, 'PARAMS_ROOT': str(root / 'params'),
         'DEBUG': '1', 'REPLAY': '1', 'SIMULATION': '1',
         'LD_PRELOAD': str(args.library), 'PARAMSD_PHASE_TRACE': str(output / 'poll.jsonl'),
         'PARAMSD_PHASE_POSITION': str(memory / 'LastGPSPosition'),
         'PARAMSD_PHASE_READY_FD': str(ready_write), 'PARAMSD_PHASE_RELEASE_FD': str(release_read)}
  for name in ('ZMQ', 'CEREAL_FAKE'):
    env.pop(name, None)
  command = [os.environ['PARAMSD_SOURCE_PYTHON'], '-P', str(Path(__file__).with_name('paramsd_live_phase_source.py')),
             str(root), str(args.oracle), str(output)]
  daemon, peer = None, None
  try:
    with (output / 'source.stdout').open('w') as stdout, (output / 'source.stderr').open('w') as stderr, (output / 'peer.stderr').open('w') as peer_error:
      daemon = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr, pass_fds=(ready_write, release_read))
      deadline = time.monotonic() + 10
      names = ('carState', 'liveCalibration', 'gpsLocationExternal', 'livePose')
      while not all((shm / name).exists() for name in (*names, 'liveParameters')):
        assert daemon.poll() is None, (output / 'source.stderr').read_text()
        assert time.monotonic() < deadline
        time.sleep(.005)
      peer_env = {key: value for key, value in env.items() if not key.startswith('PARAMSD_PHASE_') and key != 'LD_PRELOAD'}
      peer = subprocess.Popen([args.target / 'debug/examples/paramsd_peer', 'gpsLocationExternal'], env=peer_env,
                              stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_error, text=True)
      assert read_line(peer) == {'ready': True}
      (params / 'CarParams').write_bytes(bytes(car_params()))
      assert select.select([ready_read], [], [], 10)[0], 'source second-poll barrier missing'
      assert os.read(ready_read, 1) == b'R'
      updates = [json.loads(line) for line in (output / 'source-updates.jsonl').read_text().splitlines()]
      assert updates == [{'frame': 0, 'updated': dict.fromkeys(('livePose', 'liveCalibration', 'carState', 'gpsLocationExternal'), False)}]
      assert all(readers_ready(shm, names, daemon.pid).values())
      before = pointer(shm / 'livePose')
      frame = frames('gpsLocationExternal', 1)[0]
      peer.stdin.write(json.dumps({'packets': frame['messages']}) + '\n')
      peer.stdin.flush()
      deadline = time.monotonic() + 2
      while pointer(shm / 'livePose') == before:
        assert peer.poll() is None
        assert time.monotonic() < deadline, 'owned first pose not queued'
        time.sleep(.001)
      os.write(release_write, b'R')
      packet = read_line(peer)['packet']
      assert daemon.wait(timeout=5) == 0, (output / 'source.stderr').read_text()
      peer.stdin.close()
      assert peer.wait(timeout=5) == 0
      updates = [json.loads(line) for line in (output / 'source-updates.jsonl').read_text().splitlines()]
      assert [row['frame'] for row in updates] == [0, 1]
      assert list((output / 'source-publication.bin').read_bytes()) == packet
      result = {'pass': True, 'source_frames': [0, 1], 'empty_updated': updates[0]['updated'],
                'completed_before_input': 1, 'queued_input_before_release': True, 'source_exit': daemon.returncode,
                'peer_exit': peer.returncode, 'argv': command}
      (output / 'result.json').write_text(json.dumps(result, indent=2))
      print(json.dumps(result))
  finally:
    for process in (peer, daemon):
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=5)
    for fd in (ready_read, ready_write, release_read, release_write):
      os.close(fd)
    shutil.rmtree(shm)


def controlled_red(args) -> None:
  spec = importlib.util.spec_from_file_location('baseline_paramsd_daemon', args.baseline_checker)
  assert spec is not None and spec.loader is not None
  checker = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(checker)
  output = args.evidence
  output.mkdir(parents=True, exist_ok=False)
  original_popen, original_ready = subprocess.Popen, checker.readers_ready
  trace = output / 'poll.jsonl'

  def observed_popen(command, **kwargs):
    if Path(command[0]).name == 'openpilot-paramsd':
      env = kwargs['env'].copy()
      env.update(LD_PRELOAD=str(args.library), PARAMSD_PHASE_TRACE=str(trace),
        PARAMSD_PHASE_POSITION=str(Path(command[-1]) / env['OPENPILOT_PREFIX'] / 'LastGPSPosition'))
      kwargs['env'] = env
    return original_popen(command, **kwargs)

  def delayed_ready(shm, names, pid):
    ready = original_ready(shm, names, pid)
    if all(ready.values()) and trace.exists():
      rows = [json.loads(line) for line in trace.read_text().splitlines()]
      if any(row['event'] == 'enter' and row['completed'] == 2 for row in rows):
        (output / 'input-release.json').write_text(json.dumps({'monotonic_ns': time.monotonic_ns(), 'completed': 2}))
        return ready
    return dict.fromkeys(names, False)

  subprocess.Popen = observed_popen
  checker.readers_ready = delayed_ready
  try:
    with tempfile.TemporaryDirectory(prefix='paramsd-controlled-') as temporary:
      try:
        checker.phase(args.target, args.oracle, (Path(temporary), output), False)
      except AssertionError as error:
        assert error.args and error.args[0][0] == '1200-frame persistence after startup poll', error
        result = {'controlled_red': True, 'original_assertion': str(error),
                  'independently_released_after_completed': 2, 'runtime_changed': False}
        (output / 'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result))
      else:
        raise AssertionError('controlled extra startup poll did not fail strict1199 equality')
  finally:
    subprocess.Popen, checker.readers_ready = original_popen, original_ready


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('mode', choices=('source', 'red'))
  parser.add_argument('--target', type=Path, required=True)
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--library', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--baseline-checker', type=Path)
  args = parser.parse_args()
  if args.mode == 'source':
    source_control(args)
  else:
    if args.baseline_checker is None:
      parser.error('red requires --baseline-checker with the unchanged original checker')
    controlled_red(args)


if __name__ == '__main__':
  main()
