#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pyzmq==27.2.0", "zstandard==0.25.0", "pytest==9.0.2"]
# ///
# Run in the original-model environment with PYTHONPATH=msgq-python:.:tinygrad_repo:rust/tools.
"""Inject source-compatible send failures at link time while real model/VisionIPC work continues."""
import argparse
import importlib.util
import json
import logging
import os
from pathlib import Path
import shutil
import subprocess
from types import SimpleNamespace
import pytest

import zmq

from check_driving_daemon import build_peer, check
from logging_producer_reference import original_socket_handler, source


def build(root: Path, output: Path) -> Path:
  shim = output / 'model_logging_fault.o'
  include = next((root / 'rust/target/debug/build').glob('zmq-sys-*/out/source/include'))
  subprocess.run(['c++', '-std=c++17', '-O2', f'-I{include}', '-c', str(root / 'rust/tools/model_logging_fault.cc'), '-o', str(shim)], check=True)
  command = ['cargo', 'rustc', '--manifest-path', str(root / 'rust/Cargo.toml'), '-p', 'openpilot-driving-modeld',
             '--bin', 'openpilot-driving-modeld', '--locked', '--', '-C', f'link-arg={shim}', '-C', 'link-arg=-Wl,--wrap=zmq_msg_send']
  binary = output / 'openpilot-driving-modeld-fault'
  try:
    subprocess.run(command, check=True)
    shutil.copyfile(root / 'rust/target/debug/openpilot-driving-modeld', binary)
    binary.chmod(0o755)
  finally:
    subprocess.run(['cargo', 'build', '--manifest-path', str(root / 'rust/Cargo.toml'), '-p', 'openpilot-driving-modeld',
                    '--bin', 'openpilot-driving-modeld', '--locked'], check=True)
  (output / 'fault-build.json').write_text(json.dumps({'command': command, 'scope': 'link wrapper only; source unchanged'}, indent=2) + '\n')
  return binary


def source_failure(root: Path, output: Path) -> dict:
  logger, _ = source()
  handler = original_socket_handler(f'ipc:///tmp/model-log-fault-source-{os.getpid()}', logger)
  handler.connect()
  handler.sock.close()
  record = logging.LogRecord('swaglog', logging.WARNING, __file__, 1, 'ordinary', (), None)
  try:
    handler.emit(record)
  except zmq.ZMQError as error:
    assert error.errno == zmq.ENOTSOCK
  else:
    raise AssertionError('ordinary source logging must propagate unexpected ZMQ errors')
  spec = importlib.util.spec_from_file_location('source_diagnostics_fault', root / 'openpilot/common/runtime_diagnostics.py')
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  logger.addHandler(handler)
  try:
    diagnostics = module.RuntimeDiagnostics('modeld', logger.event, interval=0.0)
    diagnostics.record(inference_ms=10.0)
    assert diagnostics.frames == 0 and diagnostics.samples == {}
    recovered = []
    diagnostics.emit = lambda _name, **fields: recovered.append(fields)
    diagnostics.record(inference_ms=20.0)
    assert recovered[0]['frames'] == 1 and recovered[0]['metrics']['inference_ms']['mean'] == 20.0
  finally:
    logger.removeHandler(handler)
    handler.close()
  result = {'ordinary_errno': zmq.ENOTSOCK, 'runtime_timing': 'exception suppressed after reset', 'recovered': recovered}
  (output / 'source-failure.json').write_text(json.dumps(result, indent=2) + '\n')
  return result


def run(args) -> None:
  root = Path(__file__).resolve().parents[2]
  args.output.mkdir(parents=True)
  binary = build(root, args.output)
  peer = args.output / 'driving-peer'
  build_peer(root, peer)
  source_result = source_failure(root, args.output)
  reports = []
  for mode in ['timing-error', 'timing-full']:
    output = args.output / mode
    output.mkdir()
    audit = output / 'audit'
    audit.mkdir()
    with pytest.MonkeyPatch.context() as patched:
      patched.setenv('MODEL_LOG_FAULT', mode)
      patched.setenv('MODEL_LOG_FAULT_AUDIT', str(audit))
      report = check(SimpleNamespace(binary=binary, catalog=args.catalog, models=args.models, collector=args.collector,
                                     output=output, peer=peer), (1344, 760), 'dual')
    faults = list(audit.glob('*.packet'))
    assert len(faults) == 1, faults
    packet = faults[0].read_bytes()
    assert packet[0] == 20
    failed = json.loads(packet[1:])
    assert failed['msg']['event'] == 'runtimeTiming'
    captured = json.loads((output / '1344-dual/logging/logMessage.json').read_text())
    following = [record['msg'] for record in captured if isinstance(record['msg'], dict)
                 and record['process'] == failed['process'] and record['msg']['mono_time'] > failed['msg']['mono_time']]
    assert following
    first = following[0]
    frames = [2, *(step['frame_id'] for step in report['trace'])]
    expected_frames = sum(failed['msg']['frame_id'] < frame <= first['frame_id'] for frame in frames)
    assert first['frames'] == expected_frames
    assert all(value['count'] == expected_frames for value in first['metrics'].values())
    reports.append({'mode': mode, 'fault_file': str(faults[0]), 'failed_frames': failed['msg']['frames'],
                    'next_interval_frames': first['frames'], 'expected_next_frames': expected_frames,
                    'model_messages': report['messages'], 'compared_model_fields': report['compared_fields']})
  audit = args.output / 'ordinary-audit'
  audit.mkdir()
  result = subprocess.run([str(binary), '--trusted-catalog', str(args.catalog)],
                          env=dict(os.environ, OPENPILOT_PREFIX='model-log-fault-' + str(os.getpid()),
                                   MODEL_LOG_FAULT='ordinary', MODEL_LOG_FAULT_AUDIT=str(audit)), capture_output=True, timeout=5)
  assert result.returncode == 1 and b'Socket operation on non-socket' in result.stderr
  (args.output / 'ordinary.stderr').write_bytes(result.stderr)
  summary = {'result': 'pass', 'source': source_result, 'native': reports, 'ordinary_exit': result.returncode,
             'scope': 'ENOTSOCK/EAGAIN are injected at the final link; actual model, VisionIPC, msgq, producer and collector run unchanged'}
  (args.output / 'report.json').write_text(json.dumps(summary, indent=2) + '\n')
  print(json.dumps(summary))


if __name__ == '__main__':
  parser = argparse.ArgumentParser()
  for name in ['catalog', 'models', 'collector', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  for name in ['catalog', 'models', 'collector', 'output']:
    setattr(args, name, getattr(args, name).resolve())
  run(args)
