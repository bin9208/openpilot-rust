#!/usr/bin/env python3
"""Native IPC/Params process checks using only synthetic private namespaces."""

import argparse
from itertools import pairwise
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import tempfile
import time
import uuid

from openpilot.cereal import log
from lagd_frames import car_params, cached, messages
from lagd_loop_reference import run
from lagd_reference import ROOT, compare, normalized


def scenario(binary, peer_binary, output, mode):
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'lagd_' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  budget = json.loads((ROOT / 'rust/crates/lagd/tests/tolerances.json').read_text())['cereal_float32']
  process = peer = None
  with tempfile.TemporaryDirectory(prefix='lagd-native-') as temporary:
    root = Path(temporary)
    params = root / prefix
    params.mkdir()
    cp = car_params()
    seed = b'corrupt' if mode == 'corrupt' else cached()
    (params / 'LiveDelay').write_bytes(seed)
    (params / 'CarParamsPrevRoute').write_bytes(cp)
    env = {key: value for key, value in os.environ.items() if key not in ['ZMQ', 'CEREAL_FAKE']}
    env.update(PARAMS_ROOT=str(root), OPENPILOT_PREFIX=prefix, DEBUG='1', SIMULATION='0' if mode == 'cadence' else '1')
    count = 1201 if mode == 'learning' else (11 if mode == 'cadence' else 6)
    publications = (count - 1) // 5 + 1
    frames = []
    observed = []
    times = []
    try:
      with (output / 'daemon.log').open('w') as stderr, (output / 'peer.log').open('w') as peer_stderr:
        peer = subprocess.Popen(
          [peer_binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=peer_stderr, text=True, bufsize=1, env=env, start_new_session=True
        )
        assert peer.stdout.readline().strip() == 'ready'
        process = subprocess.Popen([binary, '--frames', str(publications)], stdout=stderr, stderr=stderr, env=env, start_new_session=True)
        time.sleep(0.15)
        assert process.poll() is None
        assert not (params / 'CarParams').exists()
        (params / 'CarParams').write_bytes(cp)
        time.sleep(0.15)
        assert Path(f'/proc/{process.pid}/exe').resolve() == binary.resolve()
        for index in range(count):
          raw = messages(index, 'recovery' if mode == 'learning' else 'normal')
          if mode == 'cadence' and index % 5:
            raw.pop(1)
          # Publish the polled livePose last, after the other services in this batch.
          raw = raw[1:] + raw[:1]
          frames.append({'messages': [list(value) for value in raw]})
          request = {'messages': [list(value) for value in raw], 'receive': index % 5 == 0}
          peer.stdin.write(json.dumps(request) + '\n')
          peer.stdin.flush()
          assert select.select([peer.stdout], [], [], 6)[0], ('peer response timeout', index)
          row = json.loads(peer.stdout.readline())
          if index % 5 == 0:
            assert row['packet'] is not None
            with log.Event.from_bytes(bytes(row['packet'])) as event:
              value = normalized(event.to_dict())
            assert 0 <= time.monotonic() - value['logMonoTime'] / 1e9 < 2
            times.append(time.monotonic())
            observed.append(value)
            if index in [0, 5, 1200]:
              (output / f'frame-{index}.capnp').write_bytes(bytes(row['packet']))
          if mode == 'cadence':
            time.sleep(0.05)
        assert process.wait(timeout=3) == 0
        with log.Event.from_bytes((params / 'LiveDelay').read_bytes()) as value:
          saved = normalized(value.to_dict())
        assert saved == observed[-1 if mode == 'learning' else 0]
        if mode == 'cadence':
          assert observed[0]['valid'] is False and all(row['valid'] for row in observed[1:])
          assert all(0.20 <= right - left <= 0.4 for left, right in pairwise(times)), times
        else:
          config = {'car': list(cp), 'saved': list(seed), 'previous': list(cp)}
          expected = [row['packet'] for row in run(frames, config) if row['publish']]
          for index, (raw, value) in enumerate(zip(expected, observed, strict=True)):
            with log.Event.from_bytes(bytes(raw)) as event:
              source = normalized(event.to_dict())
            value = dict(value, logMonoTime=123456789)
            compare(source, value, budget, f'{mode}-publication-{index}')
          if mode == 'corrupt':
            assert 'Failed to retrieve initial lag' in (output / 'daemon.log').read_text()
          else:
            assert observed[-1]['liveDelay']['validBlocks'] > 5
            assert observed[-1]['valid'] is False
        (output / 'publications.json').write_text(json.dumps(observed, indent=2) + '\n')
        (output / 'result.json').write_text(
          json.dumps(
            {
              'mode': mode,
              'input_frames': count,
              'publications': len(observed),
              'returncode': process.returncode,
              'native_executable': str(binary.resolve()),
              'carparams_wait': True,
              'persisted_frame': 1200 if mode == 'learning' else 0,
              'intervals': [b - a for a, b in pairwise(times)],
              'pass': True,
            },
            indent=2,
          )
          + '\n'
        )
    finally:
      for child in [process, peer]:
        if child is not None:
          if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
          child.wait(timeout=3)
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)
  print('PASS: native ' + mode, flush=True)


def wait_shutdown(binary, output):
  with tempfile.TemporaryDirectory(prefix='lagd-stop-') as temporary:
    prefix = 'lagd_' + uuid.uuid4().hex
    shm = Path('/dev/shm/msgq_' + prefix)
    shm.mkdir()
    env = dict(os.environ, PARAMS_ROOT=temporary, OPENPILOT_PREFIX=prefix)
    process = subprocess.Popen([binary], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    try:
      time.sleep(0.15)
      assert process.poll() is None
      process.send_signal(signal.SIGTERM)
      stdout, stderr = process.communicate(timeout=3)
      assert process.returncode == 0, stderr
      output.write_text(json.dumps({'returncode': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode(), 'pass': True}) + '\n')
    finally:
      if process.poll() is None:
        process.kill()
      process.wait(timeout=3)
      shutil.rmtree(shm)
  print('PASS: signal while waiting for CarParams', flush=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('peer', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  for mode in ['learning', 'corrupt', 'cadence']:
    scenario(args.binary, args.peer, args.output / mode, mode)
  wait_shutdown(args.binary, args.output / 'wait-signal.json')


if __name__ == '__main__':
  main()
