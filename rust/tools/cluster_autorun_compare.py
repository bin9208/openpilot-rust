#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq", "numpy"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_autorun_compare.py ROOT BINDING NATIVE OUTPUT
"""Compare persistent autorun Params owners using the original binding and helpers."""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys


def source(root: Path, binding: Path, case_path: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'rust/tools')]
  from original_params_binding import load

  case = json.loads(case_path.read_text())
  module, logging = load(binding, 'ipc://' + str(case_path.parent / 'source-log'), case_path.parent / 'logs')
  path = root / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  tree = ast.parse(path.read_text())
  names = {'_read_hud_mode', '_read_hud_debug_mode', '_read_is_onroad', '_hud_output_allowed', '_read_encoder_mode', '_read_orientation'}
  constants = {
    'HUD_PARAM',
    'HUD_DEBUG_PARAM',
    'HUD_ENCODER_PARAM',
    'HUD_ORIENTATION_PARAM',
    'IS_ONROAD_PARAM',
    'ENCODER_AUTO',
    'ENCODER_JPEG',
    'ENCODER_HARDWARE',
    'ENCODER_SOFTWARE',
    'ENCODER_NAMES',
  }
  nodes = [
    node
    for node in tree.body
    if isinstance(node, ast.FunctionDef)
    and node.name in names
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id in constants for target in node.targets)
  ]
  assert {node.name for node in nodes if isinstance(node, ast.FunctionDef)} == names
  autorun = {'Params': module.Params}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), autorun)
  params = module.Params()
  observer = module.Params()
  observed = []
  for action in case['actions']:
    match action['op']:
      case 'settings':
        settings = {
          'hud_mode': autorun['_read_hud_mode'](params),
          'encoder': autorun['ENCODER_NAMES'][autorun['_read_encoder_mode'](params)],
          'orientation': autorun['_read_orientation'](params),
        }
        observed.append({'settings': settings, 'allowed': autorun['_hud_output_allowed'](params)})
      case 'initialize':
        params.put_bool_nonblocking('ClusterHudConnected', False)
        observed.append(None)
      case 'remove_namespace':
        shutil.rmtree(Path(case['root']) / case['prefix'])
        observed.append(None)
      case _:
        raise AssertionError(action)
  del params
  result = {'observed': observed, 'connected': observer.get_bool('ClusterHudConnected'), 'namespace_exists': (Path(case['root']) / case['prefix']).is_dir()}
  logging.ipchandler.close()
  print(json.dumps(result))


def compare(root: Path, binding: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  rows = []
  for name, values, directory, removed in [
    ('settings', [b'1', b'2', b'2', b'0', b'1'], None, False),
    ('malformed-hud', [b'x', b'0', b'0', b'0', b'1'], None, False),
    ('malformed-encoder', [b'1', b'x', b'0', b'0', b'1'], None, False),
    ('malformed-orientation', [b'1', b'0', b'x', b'0', b'1'], None, False),
    ('malformed-debug', [b'1', b'0', b'0', b'x', b'1'], None, False),
    ('malformed-overflow', [b'2147483648', b'0', b'0', b'0', b'1'], None, False),
    ('unsupported-encoder', [b'1', b'44', b'0', b'1', b'0'], None, False),
    ('prefix-integers', [b' +003tail', b'2.7', b'2tail', b'1tail', b'0'], None, False),
    ('absent', [None] * 5, None, False),
    ('directory-hud', [None] * 5, 'ClusterHud', False),
    ('removed-namespace', [b'1', b'2', b'2', b'0', b'1'], None, True),
  ]:
    results = []
    for implementation in ['source', 'native']:
      destination = output / name / implementation
      namespace = destination / 'params/fixture'
      namespace.mkdir(parents=True)
      for key, value in zip(['ClusterHud', 'ClusterHudEncoder', 'ClusterHudOrientation', 'ClusterHudDebug', 'IsOnroad'], values, strict=True):
        if value is not None:
          (namespace / key).write_bytes(value)
      (namespace / 'ClusterHudConnected').write_bytes(b'1')
      if directory:
        (namespace / directory).mkdir()
      actions = [{'op': 'settings'}]
      if removed:
        actions.extend([{'op': 'remove_namespace'}, {'op': 'settings'}])
      else:
        actions.append({'op': 'initialize'})
      case = {'root': str(namespace.parent), 'prefix': 'fixture', 'actions': actions}
      data = json.dumps(case)
      case_path = destination / 'input.json'
      case_path.write_text(data + '\n')
      environment = {**os.environ, 'PARAMS_ROOT': str(namespace.parent), 'OPENPILOT_PREFIX': 'fixture'}
      argv = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_path)] if implementation == 'source' else [str(native)]
      )
      completed = subprocess.run(
        argv, env=environment, input=None if implementation == 'source' else data, capture_output=True, text=True, timeout=20, check=False
      )
      receipt = {'argv': argv, 'input': case, 'returncode': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      if name.startswith('malformed-'):
        assert completed.returncode == -signal.SIGABRT, receipt
        results.append({'fatal_signal': 'SIGABRT'})
      else:
        assert completed.returncode == 0, receipt
        results.append(json.loads(completed.stdout.splitlines()[-1]))
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'pass': results[0] == results[1]}
    (output / name / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    rows.append(row)
  result = {
    'pass': all(row['pass'] for row in rows),
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'original_params_sha256': hashlib.sha256(binding.read_bytes()).hexdigest(),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['pass']


if __name__ == '__main__':
  match sys.argv[1:]:
    case ['--source', root, binding, case_path]:
      source(*(Path(value).resolve() for value in [root, binding, case_path]))
    case [root, binding, native, output]:
      compare(*(Path(value).resolve() for value in [root, binding, native, output]))
    case _:
      raise SystemExit('ROOT BINDING NATIVE OUTPUT | --source ROOT BINDING CASE')
