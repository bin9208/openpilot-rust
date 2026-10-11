#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pyzmq", "pyusb==1.3.1", "pycryptodome"]
# ///
# ─── How to run ───
# python3 rust/tools/cluster_usb_dim_compare.py ROOT BINDING ORACLE LIBRARY NATIVE OUTPUT
"""Compare source and native autorun dimming through real Params/clocks and an owned USB recipient."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from cluster_usb_runtime_compare import payloads


def compare(root: Path, binding: Path, oracle: Path, library: Path, native: Path, output: Path) -> None:
  output.mkdir(parents=True)
  rows = []
  for name, mode, missing_vendor in [
    ('dim-ok', 'ok', False),
    ('dim-sync-retry', 'sync_timeout_once', False),
    ('dim-missing-device', 'missing', False),
    ('dim-config-error', 'config_error', False),
    ('dim-missing-vendor', 'ok', True),
  ]:
    directory = output / name
    results, fingerprints, wires = [], [], []
    for implementation in ['source', 'native']:
      destination = directory / implementation
      namespace = destination / 'params/fixture'
      namespace.mkdir(parents=True)
      (namespace / 'ClusterHudConnected').write_bytes(b'0')
      repository = destination / 'missing-vendor' if missing_vendor else root
      case = {'root': str(namespace.parent), 'repository': str(repository), 'lock': str(destination / 'bus.lock'), 'product': 0x92, 'dim': True}
      case_path = destination / 'input.json'
      case_path.write_text(json.dumps(case) + '\n')
      environment = {
        **os.environ,
        'PARAMS_ROOT': str(namespace.parent),
        'OPENPILOT_PREFIX': 'fixture',
        'TZ': 'UTC',
        'USB_FIXTURE_MODE': mode,
        'USB_FIXTURE_WIRE': str(destination / 'wire.txt'),
        'USB_FIXTURE_BYTES': str(destination / 'bytes.bin'),
        'USB_FIXTURE_LOG': str(destination / 'libusb.log'),
        'LD_LIBRARY_PATH': str(library.parent),
      }
      source = root / 'rust/tools/cluster_usb_runtime_compare.py'
      argv = (
        [sys.executable, str(source), '--source', str(root), str(binding), str(oracle), str(library), str(case_path)]
        if implementation == 'source'
        else [str(native)]
      )
      clock = [sys.executable, str(root / 'rust/tools/cluster_params_runtime_compare.py'), '--clock', str(root)]
      before = subprocess.run(clock, env=environment, capture_output=True, text=True, check=True)
      completed = subprocess.run(
        argv, input=None if implementation == 'source' else json.dumps(case), env=environment, capture_output=True, text=True, timeout=20, check=False
      )
      after = subprocess.run(clock, env=environment, capture_output=True, text=True, check=True)
      bounds = (json.loads(before.stdout), json.loads(after.stdout))
      receipt = {
        'argv': argv,
        'input': case,
        'returncode': completed.returncode,
        'stdout': completed.stdout,
        'stderr': completed.stderr,
        'clock_bounds': bounds,
      }
      (destination / 'invocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
      assert completed.returncode == 0, receipt
      results.append(json.loads(completed.stdout.splitlines()[-1]))
      captured = destination / 'bytes.bin'
      fingerprints.append(payloads(captured, bounds) if captured.exists() else [])
      wire = destination / 'wire.txt'
      wires.append(wire.read_text() if wire.exists() else '')
      assert (namespace / 'ClusterHudConnected').read_bytes() == b'0'
      cleanup = destination / 'libusb.log'
      if cleanup.exists():
        assert cleanup.read_text().endswith('exit clean\n'), cleanup.read_text()
    row = {
      'scenario': name,
      'source': results[0],
      'native': results[1],
      'normalized_payload_sha256': fingerprints,
      'wire': wires,
      'pass': results[0] == results[1] and fingerprints[0] == fingerprints[1] and wires[0] == wires[1],
    }
    rows.append(row)
    (directory / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
  result = {
    'rows': rows,
    'pass': all(row['pass'] for row in rows),
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': (
      'actual original dimming helper, Cython/native Params, wall-clock brackets and owned USB ABI; '
      + 'exact payloads after timestamp removal and exact wire order; no physical display'
    ),
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['pass']


if __name__ == '__main__':
  compare(*(Path(value).resolve() for value in sys.argv[1:]))
