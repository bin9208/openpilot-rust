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
from paramsd_fixture import car_params, event


def run(binary, evidence, name, seed):
  output = evidence / name
  output.mkdir(parents=True, exist_ok=True)
  prefix = 'rust-probe-params-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  with tempfile.TemporaryDirectory(prefix='paramsd-startup-') as temporary:
    root = Path(temporary)
    params, memory = root / 'params' / prefix, root / 'memory' / prefix
    params.mkdir(parents=True)
    memory.mkdir(parents=True)
    (memory / 'LastGPSPosition').write_bytes(b'previous')
    for key, value in seed.items():
      (params / key).write_bytes(bytes(value))
    environment = os.environ | {'OPENPILOT_PREFIX': prefix, 'PARAMS_ROOT': str(root / 'params'), 'DEBUG': '1', 'REPLAY': '1'}
    for key in ('ZMQ', 'CEREAL_FAKE'):
      environment.pop(key, None)
    process = None
    try:
      with (output / 'stdout.log').open('w') as stdout, (output / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen([binary, '--memory-root', root / 'memory'], env=environment, stdout=stdout, stderr=stderr)
        failure = name in ('malformed-car', 'wrong-covariance-dimensions')
        if failure:
          assert process.wait(timeout=10) == 1
        else:
          deadline = time.monotonic() + 10
          while not (shm / 'liveParameters').exists() or (name != 'blocked-car' and (memory / 'LastGPSPosition').exists()):
            assert process.poll() is None, (output / 'stderr.log').read_text()
            assert time.monotonic() < deadline
            time.sleep(.01)
          assert process.poll() is None
          process.send_signal(signal.SIGTERM)
          assert process.wait(timeout=5) == 0
        if name == 'blocked-car':
          assert (memory / 'LastGPSPosition').read_bytes() == b'previous'
        if name in ('invalid-json', 'migration'):
          assert (params / 'LiveParameters').exists()
        if name == 'wrong-fields':
          assert not (params / 'LiveParameters').exists()
        if name == 'malformed-cache':
          assert not (params / 'LiveParametersV2').exists()
        if name == 'wrong-covariance-dimensions':
          assert (params / 'LiveParametersV2').read_bytes() == bytes(seed['LiveParametersV2'])
          assert 'dimensions' in (output / 'stderr.log').read_text()
        if name == 'migration':
          value = (params / 'LiveParametersV2').read_bytes()
          (output / 'migrated.bin').write_bytes(value)
          with log.Event.from_bytes(value) as message:
            assert not message.valid and message.liveParameters.valid
            assert message.liveParameters.steerRatio == 17.
        result = {'pass': True, 'scenario': name, 'exit': process.returncode,
                  'files': sorted(path.name for path in params.iterdir())}
        (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
        return result
    finally:
      if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=5)
      shutil.rmtree(shm)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  base = {'CarParams': car_params(), 'CarParamsPrevRoute': car_params()}
  scenarios = {
    'blocked-car': {}, 'malformed-car': {'CarParams': [1, 2]},
    'migration': base | {'LiveParameters': list(b'{"steerRatio":17,"stiffnessFactor":0.7,"angleOffsetAverageDeg":2}')},
    'invalid-json': base | {'LiveParameters': list(b'{')}, 'wrong-fields': base | {'LiveParameters': list(b'{}')},
    'malformed-cache': base | {'LiveParametersV2': [1, 2]},
    'wrong-covariance-dimensions': base | {'LiveParametersV2': event('liveParameters', 1., {'steerRatio': 15., 'debugFilterState': {'std': [.2, .3]}})},
  }
  results = [run(args.binary.resolve(), args.evidence, name, seed) for name, seed in scenarios.items()]
  (args.evidence / 'results.json').write_text(json.dumps({'pass': True, 'scenarios': results}, indent=2) + '\n')
  print(json.dumps({'pass': True, 'scenarios': len(results)}))


if __name__ == '__main__':
  main()
