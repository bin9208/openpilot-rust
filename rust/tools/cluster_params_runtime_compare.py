#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq", "numpy"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_params_runtime_compare.py ROOT BINDING NATIVE OUTPUT
"""Drive physical Params through original Cython helpers and the native cluster runtime."""

from __future__ import annotations

import ast
import hashlib
import importlib
import itertools
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time


def clock(root: Path) -> int:
  vendor = root / 'openpilot/selfdrive/carrot/cluster/.vendor/turing-smart-screen-python-main/library/lcd/lcd_comm_turing_usb.py'
  tree = ast.parse(vendor.read_text())
  nodes = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'build_command_packet_header']
  assert len(nodes) == 1
  namespace = {'time': time, 'struct': struct}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(vendor), 'exec'), namespace)
  return struct.unpack('<I', namespace['build_command_packet_header'](10)[4:8])[0]


def original(root: Path, binding: Path, case_file: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'openpilot/selfdrive/carrot/cluster'), str(root / 'rust/tools')]
  from original_params_binding import load

  case = json.loads(case_file.read_text())
  module, logging = load(binding, 'ipc://' + str(case_file.parent / 'source-log'), case_file.parent / 'logs')
  display = importlib.import_module('cluster_usb_display')
  params = module.Params()
  values = []
  for action in case['actions']:
    match action['op']:
      case 'flags':
        values.append([display._usbgpu_transfer_active(), display._usbgpu_reset_protected()])
      case 'connected':
        display._set_cluster_hud_connected(action['value'])
        values.append(params.get_bool('ClusterHudConnected'))
      case 'milliseconds':
        values.append(clock(root))
      case 'remove_namespace':
        shutil.rmtree(Path(case['root']) / case['prefix'])
        values.append(None)
      case 'replace_root_file':
        shutil.rmtree(case['root'])
        Path(case['root']).write_bytes(b'ordinary file')
        values.append(None)
      case _:
        raise AssertionError(action)
  logging.ipchandler.close()
  print(json.dumps(values))


def lifecycle(root: Path, binding: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  rows = []
  for name, actions in [
    ('removed-namespace', [{'op': 'remove_namespace'}, {'op': 'connected', 'value': True}]),
    ('root-ordinary-file', [{'op': 'replace_root_file'}, {'op': 'flags'}]),
  ]:
    destination = output / name
    results = []
    for implementation in ['source', 'native']:
      params = destination / implementation / 'params'
      namespace = params / 'fixture'
      namespace.mkdir(parents=True)
      case = {'root': str(params), 'prefix': 'fixture', 'actions': actions}
      case_file = params.parent / 'input.json'
      case_file.write_text(json.dumps(case) + '\n')
      environment = {**os.environ, 'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'fixture'}
      command = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_file)] if implementation == 'source' else [str(native)]
      )
      completed = subprocess.run(
        command, env=environment, input=None if implementation == 'source' else json.dumps(case), capture_output=True, text=True, timeout=20, check=False
      )
      receipt = {
        'argv': command,
        'input': case,
        'returncode': completed.returncode,
        'stdout': completed.stdout,
        'stderr': completed.stderr,
      }
      (params.parent / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      results.append(json.loads(completed.stdout.splitlines()[-1]))
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'pass': results[0] == results[1]}
    (destination / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    rows.append(row)
  result = {
    'pass': all(row['pass'] for row in rows),
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'original_params_sha256': hashlib.sha256(binding.read_bytes()).hexdigest(),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  if not result['pass']:
    raise SystemExit(1)


def compare(root: Path, binding: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  rows = []
  values = [None, b'1', b'0', b'', b' 1', b'\xff']
  cases = [(f'flags-{index}', active, loading, None) for index, (active, loading) in enumerate(itertools.product(values, repeat=2))]
  cases.extend([('directory-active', None, None, 'UsbGpuActive'), ('directory-loading', None, None, 'UsbGpuLoading')])
  for name, active, loading, directory_key in cases:
    destination = output / name
    destination.mkdir()
    results = []
    for implementation in ['source', 'native']:
      params = destination / implementation / 'params'
      namespace = params / 'fixture'
      namespace.mkdir(parents=True)
      for key, value in [('UsbGpuActive', active), ('UsbGpuLoading', loading)]:
        if value is not None:
          (namespace / key).write_bytes(value)
      if directory_key:
        (namespace / directory_key).mkdir()
      case = {
        'root': str(params),
        'prefix': 'fixture',
        'actions': [{'op': 'flags'}, {'op': 'connected', 'value': True}, {'op': 'connected', 'value': False}, {'op': 'connected', 'value': True}],
      }
      data = json.dumps(case)
      case_file = destination / implementation / 'input.json'
      case_file.write_text(data + '\n')
      environment = {**os.environ, 'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'fixture'}
      command = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_file)] if implementation == 'source' else [str(native)]
      )
      completed = subprocess.run(
        command, env=environment, input=None if implementation == 'source' else data, capture_output=True, text=True, timeout=20, check=False
      )
      receipt = {
        'argv': command,
        'environment': {'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'fixture'},
        'returncode': completed.returncode,
        'stdout': completed.stdout,
        'stderr': completed.stderr,
      }
      (destination / implementation / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      observed = json.loads(completed.stdout.splitlines()[-1])
      assert observed[1:] == [True, False, True], (name, observed)
      assert (namespace / 'ClusterHudConnected').read_bytes() == b'1'
      results.append(observed)
    assert results[0] == results[1], (name, results)
    (destination / 'result.json').write_text(json.dumps({'source': results[0], 'native': results[1], 'pass': True}, indent=2) + '\n')
    rows.append({'scenario': name, 'pass': True})
  for zone in ['UTC', 'Asia/Seoul', 'America/New_York']:
    destination = output / ('clock-' + zone.replace('/', '-'))
    destination.mkdir()
    namespace = destination / 'params/fixture'
    namespace.mkdir(parents=True)
    case = {'root': str(namespace.parent), 'prefix': 'fixture', 'actions': [{'op': 'milliseconds'}]}
    env = {**os.environ, 'TZ': zone}
    # Source brackets the real native process using the actual vendor clock expression.
    before = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--clock', str(root)], env=env, capture_output=True, text=True, check=True)
    actual = subprocess.run([str(native)], input=json.dumps(case), env=env, capture_output=True, text=True, check=True)
    after = subprocess.run([sys.executable, str(Path(__file__).resolve()), '--clock', str(root)], env=env, capture_output=True, text=True, check=True)
    low, observed, high = json.loads(before.stdout), json.loads(actual.stdout)[0], json.loads(after.stdout)
    assert low <= observed <= high, (zone, low, observed, high)
    (destination / 'invocation.json').write_text(
      json.dumps(
        {
          'source_before': {'argv': before.args, 'stdout': before.stdout},
          'native': {'argv': actual.args, 'input': case, 'stdout': actual.stdout},
          'source_after': {'argv': after.args, 'stdout': after.stdout},
          'TZ': zone,
          'pass': True,
        },
        indent=2,
      )
      + '\n'
    )
    rows.append({'scenario': 'live-clock-' + zone, 'pass': True})
  result = {
    'pass': True,
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'original_params_sha256': hashlib.sha256(binding.read_bytes()).hexdigest(),
    'scope': (
      'actual owned Params files and unchanged source helpers/Cython; live clock brackets in three timezones; '
      + 'not historical DST or autorun/render/runtime proof'
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  match sys.argv[1:]:
    case ['--source', root, binding, case_file]:
      original(Path(root), Path(binding), Path(case_file))
    case ['--clock', root]:
      print(json.dumps(clock(Path(root))))
    case ['--lifecycle', root, binding, native, output]:
      lifecycle(*(Path(value).resolve() for value in [root, binding, native, output]))
    case [root, binding, native, output]:
      compare(*(Path(value).resolve() for value in [root, binding, native, output]))
    case _:
      raise SystemExit('ROOT BINDING NATIVE OUTPUT | --source ROOT BINDING CASE | --clock ROOT | --lifecycle ROOT BINDING NATIVE OUTPUT')
