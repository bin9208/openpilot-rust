#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyzmq", "numpy"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_controller_compare.py ROOT BINDING NATIVE OUTPUT
"""Compare autorun state machines at a controlled entrypoint using real Params and datagrams."""

from __future__ import annotations

import ast
import hashlib
import importlib
import json
import locale
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import threading
import time
import traceback
from types import ModuleType, SimpleNamespace


def source(root: Path, binding: Path, case_path: Path) -> None:
  sys.path[:0] = [str(root), str(root / 'openpilot'), str(root / 'rust/tools'), str(root / 'openpilot/selfdrive/carrot/cluster')]
  from original_params_binding import load

  case = json.loads(case_path.read_text())
  params_module, logging = load(binding, 'ipc://' + str(case_path.parent / 'source-log'), case_path.parent / 'logs')
  display = importlib.import_module('cluster_usb_display')
  path = root / 'openpilot/selfdrive/carrot/cluster_autorun.py'
  tree = ast.parse(path.read_text())
  nodes = [node for node in tree.body if isinstance(node, (ast.FunctionDef, ast.Assign))]
  namespace = {
    '__file__': str(path),
    'Params': params_module.Params,
    'TICI': case['board'],
    'Path': Path,
    'locale': locale,
    'os': os,
    'select': select,
    'socket': socket,
    'sys': sys,
    'time': time,
    'traceback': traceback,
  }
  exec(compile(ast.Module(body=nodes, type_ignores=[]), str(path), 'exec'), namespace)
  trace = []
  found = iter(case['found'])
  entries = iter(case['entries'])
  writer = params_module.Params()

  class Scheduler:
    def __init__(self, core, enabled):
      assert core == 7 and enabled == case['board']

    def update(self, onroad, force):
      assert onroad is False and force is True
      trace.append('offroad')

  def find(product):
    trace.append({'find': product})
    return product if next(found, False) else None

  def dim(product, reason):
    trace.append({'dim': product, 'reason': reason})
    return True

  def events():
    trace.append('events')
    reader, sender = socket.socketpair(socket.AF_UNIX, socket.SOCK_DGRAM)
    reader.setblocking(False)
    for packet in case['packets']:
      sender.send(bytes(packet))
    sender.close()
    return reader

  h264_path = root / 'openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py'
  h264_tree = ast.parse(h264_path.read_text())
  classes = [node for node in h264_tree.body if isinstance(node, ast.ClassDef) and node.name == 'H264PipelineInitializationError']
  h264 = ModuleType('cluster_h264_pipeline')
  exec(compile(ast.Module(body=classes, type_ignores=[]), str(h264_path), 'exec'), h264.__dict__)

  def run_entry(exit_on_error):
    assert exit_on_error is False
    trace.append({'entry': sys.argv[1:]})
    entry = next(entries)
    for key, value in entry['values']:
      writer.put(key, int(value))
    match entry['result']:
      case 'success':
        return
      case 'h264':
        raise h264.H264PipelineInitializationError('owned H264 initialization failure')
      case 'runtime':
        raise RuntimeError('owned runtime failure')
      case 'disconnected':
        raise RuntimeError('USB display disconnected')
      case _:
        raise AssertionError(entry)

  namespace['_wait_for_usbgpu_startup'] = lambda params: trace.append('startup')
  namespace['_turn_off_supported_usb_device'] = dim
  namespace['_open_usb_uevent_socket'] = events
  display.find_supported_usb_product = find
  sys.modules['openpilot.common.realtime'] = SimpleNamespace(drop_realtime=lambda: None)
  sys.modules['openpilot.common.display_scheduling'] = SimpleNamespace(DisplayScheduler=Scheduler)
  sys.modules['cluster_h264_pipeline'] = h264
  sys.modules['selfdrive.carrot.cluster_run'] = SimpleNamespace(main=run_entry)
  update = case['update']
  worker = None
  if update:

    def apply_update():
      time.sleep(update['delay_ms'] / 1000)
      for key, value in update['values']:
        writer.put(key, int(value))

    worker = threading.Thread(target=apply_update)
    worker.start()
  started = time.monotonic()
  namespace['main']()
  elapsed = (time.monotonic() - started) * 1000
  if worker:
    worker.join()
  logging.ipchandler.close()
  print(json.dumps({'trace': trace, 'connected': writer.get_bool('ClusterHudConnected'), 'elapsed_ms': elapsed}))


def compare(root: Path, binding: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  success = {'result': 'success', 'values': [['ClusterHud', '0']]}
  h264 = {'result': 'h264', 'values': []}
  packet = list(b'ACTION=add\x00SUBSYSTEM=usb\x00PRODUCT=1cbe/0092/1\x00')
  cases = [
    ('disabled-mode', False, '0', '0', '1', [], [], [], None),
    ('pc-window', False, '1', '0', '1', [False], [success], [], None),
    ('board-auto-fallback', True, '1', '0', '1', [True], [h264, h264, success], [], None),
    ('pc-auto-fallback', False, '1', '0', '1', [True], [h264, success], [], None),
    ('forced-h264-retry', True, '1', '2', '1', [True], [{'result': 'h264', 'values': [['ClusterHud', '0']]}], [], None),
    ('usb-unplug-retry', True, '1', '1', '1', [True], [{'result': 'disconnected', 'values': [['ClusterHud', '0']]}], [], None),
    ('output-disabled-dim', True, '1', '1', '0', [], [], [], {'delay_ms': 250, 'values': [['ClusterHud', '0']]}),
    ('board-event-connect', True, '1', '1', '1', [False, True], [success], [packet], None),
    ('board-wait-stop', True, '1', '1', '1', [False], [], [], {'delay_ms': 250, 'values': [['ClusterHud', '0']]}),
    ('orientation-change', True, '1', '1', '1', [True, True], [{'result': 'success', 'values': [['ClusterHudOrientation', '2']]}, success], [], None),
  ]
  rows = []
  for name, board, hud, encoder, allowed, found, entries, packets, update in cases:
    results = []
    for implementation in ['source', 'native']:
      destination = output / name / implementation
      params = destination / 'params'
      namespace = params / 'fixture'
      namespace.mkdir(parents=True)
      values = {
        'ClusterHud': hud,
        'ClusterHudEncoder': encoder,
        'ClusterHudOrientation': '0',
        'ClusterHudDebug': allowed,
        'IsOnroad': '0',
        'UsbGpuActive': '1',
        'ClusterHudConnected': '1',
      }
      for key, value in values.items():
        (namespace / key).write_text(value)
      case = {'root': str(params), 'prefix': 'fixture', 'board': board, 'found': found, 'entries': entries, 'packets': packets, 'update': update}
      case_path = destination / 'input.json'
      case_path.write_text(json.dumps(case) + '\n')
      argv = (
        [sys.executable, str(Path(__file__).resolve()), '--source', str(root), str(binding), str(case_path)] if implementation == 'source' else [str(native)]
      )
      completed = subprocess.run(
        argv,
        input=None if implementation == 'source' else json.dumps(case),
        env={**os.environ, 'PARAMS_ROOT': str(params), 'OPENPILOT_PREFIX': 'fixture'},
        capture_output=True,
        text=True,
        timeout=15,
        check=False,
      )
      receipt = {'argv': argv, 'returncode': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      results.append(json.loads(completed.stdout.splitlines()[-1]))
    timings = [value.pop('elapsed_ms') for value in results]
    if name in {'forced-h264-retry', 'usb-unplug-retry'}:
      assert all(4950 <= elapsed <= 7000 for elapsed in timings), timings
    row = {'scenario': name, 'source': results[0], 'native': results[1], 'elapsed_ms': timings, 'pass': results[0] == results[1]}
    rows.append(row)
    (output / name / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
  result = {
    'rows': rows,
    'pass': all(row['pass'] for row in rows),
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'original_params_sha256': hashlib.sha256(binding.read_bytes()).hexdigest(),
    'scope': (
      'unchanged source controller and encoder dispatch with controlled renderer/scheduler/USB-discovery boundaries; '
      + 'actual Params, real retry clocks and owned datagrams; not a rendered runtime'
    ),
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
