#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq", "numpy", "requests"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_gpu_readiness_compare.py ROOT BINDING NATIVE MODEL ASSETS OUTPUT
"""Compose original startup waits with owned sysfs/model files and native admission."""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def source(root: Path, binding: Path, case_path: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'rust/tools')]
  from original_params_binding import load
  from openpilot.selfdrive.modeld import helpers

  case = json.loads(case_path.read_text())
  module, logging = load(binding, 'ipc://' + str(case_path.parent / 'source-log'), case_path.parent / 'logs')
  params = module.Params()
  probe = case['actions'][0]
  helpers.MODELS_DIR = Path(probe['repository']) / 'openpilot/selfdrive/modeld/models'
  helpers.Path = lambda value: Path(probe['sysfs']) if str(value) == '/sys/bus/usb/devices' else Path(value)
  path = root / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  tree = ast.parse(path.read_text())
  names = {'_usbgpu_startup_expected', '_wait_for_usbgpu_startup'}
  constants = {
    node.targets[0].id
    for node in tree.body
    if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name) and node.targets[0].id.startswith('USBGPU_')
  }
  nodes = [
    node
    for node in tree.body
    if isinstance(node, ast.FunctionDef)
    and node.name in names
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id in constants for target in node.targets)
  ]
  namespace = {'time': time}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  original = namespace['_usbgpu_startup_expected']
  observed = [original()]
  calls = 0

  def inspect():
    nonlocal calls
    calls += 1
    return original()

  namespace['_usbgpu_startup_expected'] = inspect
  namespace['_wait_for_usbgpu_startup'](params)
  observed.append({'probes': calls})
  logging.ipchandler.close()
  print(json.dumps({'observed': observed, 'connected': False, 'namespace_exists': True}))


def compare(root: Path, binding: Path, native: Path, model: Path, assets: Path, output: Path) -> None:
  assert shutil.disk_usage(output.parent).free >= (25 * 1024 + 5120) * 1024**2
  output.mkdir(parents=True)
  repository = output / 'repository'
  models = repository / 'openpilot/selfdrive/modeld/models'
  models.mkdir(parents=True)
  target = models / model.name
  shutil.copyfile(model, target)
  shutil.copyfile(model, Path(str(target) + '.chunk01of01'))
  Path(str(target) + '.chunkmanifest').write_text('1')
  selected = hashlib.sha256(b'owned-small-source-onnx-identity').hexdigest()
  active = {'model_id': 'small', 'filename': 'owned.onnx', 'size': 32, 'sha256': selected, 'url': 'https://fixture.test/owned.onnx'}
  future = {'model_id': 'future', 'filename': 'future.onnx', 'size': 3, 'sha256': 'b' * 64, 'url': 'https://fixture.test/future.onnx'}
  rows = []
  for name, speed, current, previous, expected in [
    ('active-super-speed-float', '5000.5', active, None, True),
    ('previous-super-speed', '10000', future, active, True),
    ('usb2-short-circuit', '480', active, None, False),
    ('no-local-provider', '5000', future, None, False),
    ('precompiled-only-blocks-previous', '5000', dict(future, filename='future.pkl'), active, False),
  ]:
    directory = output / name
    cache = directory / 'cache'
    cache.mkdir(parents=True)
    (cache / 'state.json').write_text(json.dumps({'active': current, 'previous': previous}))
    (cache / ('owned-' + selected[:16] + '.onnx')).write_bytes(b'owned-small-source-onnx-identity')
    (cache / ('future-' + 'b' * 16 + ('.pkl' if current['filename'].endswith('.pkl') else '.onnx'))).write_bytes(b'xxx')
    sysfs = directory / 'sysfs'
    device = sysfs / 'owned-device'
    device.mkdir(parents=True)
    for key, value in [('idVendor', 'add1'), ('idProduct', '0001'), ('speed', speed)]:
      (device / key).write_text(value)
    results = []
    for implementation in ['source', 'native']:
      destination = directory / implementation
      namespace = destination / 'params/fixture'
      namespace.mkdir(parents=True)
      (namespace / 'UsbGpuStartupFailed').write_bytes(b'1')
      action = {'repository': str(repository), 'assets': str(assets), 'sysfs': str(sysfs)}
      case = {'root': str(namespace.parent), 'prefix': 'fixture', 'actions': [dict(action, op='startup_expected'), dict(action, op='wait_gpu_filesystem')]}
      case_path = destination / 'input.json'
      case_path.write_text(json.dumps(case) + '\n')
      environment = {**os.environ, 'CARROT_BIG_MODEL_DIR': str(cache), 'PARAMS_ROOT': str(namespace.parent), 'OPENPILOT_PREFIX': 'fixture'}
      argv = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_path)] if implementation == 'source' else [str(native)]
      )
      completed = subprocess.run(
        argv, input=None if implementation == 'source' else json.dumps(case), env=environment, capture_output=True, text=True, timeout=20, check=False
      )
      receipt = {
        'argv': argv,
        'environment': {key: environment[key] for key in ['CARROT_BIG_MODEL_DIR', 'PARAMS_ROOT', 'OPENPILOT_PREFIX']},
        'returncode': completed.returncode,
        'stdout': completed.stdout,
        'stderr': completed.stderr,
      }
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      result = json.loads(completed.stdout.splitlines()[-1])
      assert result['observed'][0] is expected, result
      results.append(result)
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'pass': results[0] == results[1]}
    rows.append(row)
    (directory / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
  result = {
    'rows': rows,
    'pass': all(row['pass'] for row in rows),
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'model_sha256': hashlib.sha256(model.read_bytes()).hexdigest(),
    'assets': str(assets),
    'scope': (
      'owned sysfs float speed plus original disk state/local chunk readiness, native package admission and actual Params wait composition; '
      + 'synthetic ONNX identity, no trained model or physical GPU acceptance'
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['pass']


if __name__ == '__main__':
  match sys.argv[1:]:
    case ['--source', root, binding, case_path]:
      source(*(Path(value).resolve() for value in [root, binding, case_path]))
    case [root, binding, native, model, assets, output]:
      compare(*(Path(value).resolve() for value in [root, binding, native, model, assets, output]))
    case _:
      raise SystemExit('ROOT BINDING NATIVE MODEL ASSETS OUTPUT | --source ROOT BINDING CASE')
