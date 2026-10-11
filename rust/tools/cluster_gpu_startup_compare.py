#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq", "numpy"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_gpu_startup_compare.py ROOT BINDING NATIVE OUTPUT
"""Drive actual Params and real-clock startup waits with controlled external readiness."""

from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def source(root: Path, binding: Path, case_path: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'rust/tools')]
  from original_params_binding import load

  case = json.loads(case_path.read_text())
  module, logging = load(binding, 'ipc://' + str(case_path.parent / 'source-log'), case_path.parent / 'logs')
  params = module.Params()
  path = root / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  tree = ast.parse(path.read_text())
  nodes = [
    node
    for node in tree.body
    if isinstance(node, ast.FunctionDef)
    and node.name == '_wait_for_usbgpu_startup'
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id.startswith('USBGPU_') for target in node.targets)
  ]
  calls = 0

  def expected() -> bool:
    nonlocal calls
    calls += 1
    return case['expected']

  namespace = {'Params': module.Params, 'time': time, '_usbgpu_startup_expected': expected}
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  print('READY', flush=True)
  started = time.perf_counter()
  namespace['_wait_for_usbgpu_startup'](params)
  result = {'elapsed_ms': (time.perf_counter() - started) * 1000, 'probes': calls}
  logging.ipchandler.close()
  print(json.dumps(result))


def compare(root: Path, binding: Path, native: Path, output: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'rust/tools')]
  from original_params_binding import load

  output.mkdir(parents=True)
  module, logging = load(binding, 'ipc://' + str(output / 'writer-log'), output / 'writer-logs')
  rows = []
  cases = [
    ('no-presence', False, {}, [], 0, 1000, ''),
    ('remembered-discovery', False, {'UsbGpuHardwareSeen': True}, [], 2900, 6000, ''),
    ('active-stable', True, {'UsbGpuActive': True}, [], 9900, 13000, 'eGPU stable;'),
    ('active-lost', True, {'UsbGpuActive': True}, [(0.35, 'UsbGpuActive', False)], 300, 1800, 'became inactive'),
    ('loading-failed', True, {'UsbGpuLoading': True}, [(0.25, 'UsbGpuStartupFailed', True), (0.25, 'UsbGpuLoading', False)], 200, 1800, 'startup failed;'),
    ('startup-timeout', True, {'UsbGpuLoading': True}, [], 59900, 64000, 'timed out after 60s'),
  ]
  for name, expected, flags, changes, low, high, marker in cases:
    results = []
    for implementation in ['source', 'native']:
      destination = output / name / implementation
      destination.mkdir(parents=True)
      params_root = destination / 'params'
      environment = {**os.environ, 'PARAMS_ROOT': str(params_root), 'OPENPILOT_PREFIX': 'fixture'}
      os.environ['PARAMS_ROOT'] = str(params_root)
      os.environ['OPENPILOT_PREFIX'] = 'fixture'
      writer = module.Params()
      for key, value in flags.items():
        writer.put_bool(key, value)
      case = {'root': str(params_root), 'prefix': 'fixture', 'expected': expected, 'actions': [{'op': 'wait_gpu', 'expected': expected}]}
      case_path = destination / 'input.json'
      case_path.write_text(json.dumps(case) + '\n')
      argv = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_path)] if implementation == 'source' else [str(native)]
      )
      process = subprocess.Popen(argv, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
      if implementation == 'native':
        process.stdin.write(json.dumps(case))
        process.stdin.close()
        process.stdin = None
      ready = process.stdout.readline()
      assert ready == 'READY\n', (ready, process.poll())
      started = time.monotonic()
      timeline = []
      for after, key, value in changes:
        time.sleep(max(0.0, after - (time.monotonic() - started)))
        writer.put_bool(key, value)
        timeline.append({'after_ms': (time.monotonic() - started) * 1000, 'key': key, 'value': value})
      process.wait(timeout=70)
      stdout, stderr = process.stdout.read(), process.stderr.read()
      receipt = {'argv': argv, 'input': case, 'returncode': process.returncode, 'stdout': ready + stdout, 'stderr': stderr, 'changes': timeline}
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert process.returncode == 0, receipt
      observed = json.loads(stdout.splitlines()[-1])
      if implementation == 'native':
        observed = observed['observed'][0]
      assert low <= observed['elapsed_ms'] <= high, (name, implementation, observed)
      assert marker in stdout, (name, implementation, stdout)
      results.append({'observed': observed, 'messages': stdout.splitlines()[:-1]})
      del writer
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'pass': results[0]['messages'] == results[1]['messages']}
    (output / name / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    assert row['pass'], row
    rows.append(row)
    print(json.dumps({'scenario': name, 'pass': True}), flush=True)
  result = {
    'pass': True,
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': (
      'actual original/Rust Params, actual original wait body and real 3/10/60s deadlines; '
      + 'external presence/readiness is controlled, not proof of that probe or full runtime'
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  logging.ipchandler.close()
  print(json.dumps(result))


if __name__ == '__main__':
  match sys.argv[1:]:
    case ['--source', root, binding, case_path]:
      source(*(Path(value).resolve() for value in [root, binding, case_path]))
    case [root, binding, native, output]:
      compare(*(Path(value).resolve() for value in [root, binding, native, output]))
    case _:
      raise SystemExit('ROOT BINDING NATIVE OUTPUT | --source ROOT BINDING CASE')
