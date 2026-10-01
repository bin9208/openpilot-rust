#!/usr/bin/env python3
"""Compare original Params/cache initialization with the real native process."""

import argparse
import ast
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import uuid

from locationd_source import load
from original_params_binding import load as load_params


def source_startup(root, source, params):
  tree = ast.parse((root / 'openpilot/selfdrive/locationd/locationd.py').read_text())
  main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  start = next(
    i
    for i, node in enumerate(main.body)
    if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name) and node.targets[0].id == 'initial_pose_data'
  )
  block = ast.Module(body=main.body[start : start + 2], type_ignores=[])
  scope = source | {'params': params, 'estimator': source['LocationEstimator'](False)}
  try:
    exec(compile(block, 'source-locationd-cache-initialization', 'exec'), scope)
  except Exception as error:
    return {'accepted': False, 'error': str(error).split('stack:')[0]}
  return {'accepted': True, 'cache_absent': scope['initial_pose_data'] is None}


def scenario(args, source, params_module, mode):
  prefix = 'rust-probe-location-cache-' + uuid.uuid4().hex
  shm = Path('/dev/shm/msgq_' + prefix)
  shm.mkdir()
  process = None
  with tempfile.TemporaryDirectory(prefix='locationd-startup-') as temporary:
    params_root = Path(temporary)
    environment = os.environ | {'PARAMS_ROOT': str(params_root), 'OPENPILOT_PREFIX': prefix, 'DEBUG': '0', 'SIMULATION': '1'}
    for name in ('ZMQ', 'CEREAL_FAKE'):
      environment.pop(name, None)
    previous = os.environ.get('OPENPILOT_PREFIX')
    os.environ['OPENPILOT_PREFIX'] = prefix
    try:
      params = params_module.Params(str(params_root))
    finally:
      if previous is None:
        os.environ.pop('OPENPILOT_PREFIX', None)
      else:
        os.environ['OPENPILOT_PREFIX'] = previous
    path = Path(params.get_param_path('LocationFilterInitialState'))
    if mode == 'directory':
      path.mkdir()
    elif mode != 'missing':
      path.write_bytes(b'' if mode == 'empty' else b'corrupt')
    expected = source_startup(args.root, source, params)
    try:
      process = subprocess.Popen([args.binary], env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
      deadline = time.monotonic() + 5
      while process.poll() is None and not (shm / 'livePose').exists() and time.monotonic() < deadline:
        time.sleep(0.01)
      if process.poll() is None:
        assert (shm / 'livePose').exists(), 'native publisher initialization timeout'
        process.send_signal(signal.SIGTERM)
      stdout, stderr = process.communicate(timeout=5)
      actual = {'accepted': process.returncode == 0, 'exit': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()}
      return {'case': mode, 'source': expected, 'native': actual, 'pass': expected['accepted'] == actual['accepted']}
    finally:
      if process is not None:
        if process.poll() is None:
          process.kill()
        process.wait(timeout=5)
      shutil.rmtree(shm)
      Path('/tmp/logmessage' + prefix).unlink(missing_ok=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.binary = args.binary.resolve()
  args.root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True, exist_ok=True)
  endpoint = 'ipc://' + str(args.output.resolve() / 'source-log')
  params_module, _ = load_params(args.binding.resolve(), endpoint, args.output.resolve() / 'source-logs')
  source, _ = load(args.oracle.resolve())
  rows = [scenario(args, source, params_module, mode) for mode in ('missing', 'empty', 'directory', 'corrupt')]
  (args.output / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
  assert all(row['pass'] for row in rows), rows
  print('PASS: four original/native location cache startup cases; empty/read errors absent, corrupt data rejected')


if __name__ == '__main__':
  main()
