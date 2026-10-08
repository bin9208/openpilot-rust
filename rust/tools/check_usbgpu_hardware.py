# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: python rust/tools/check_usbgpu_hardware.py BIN_DIR EVIDENCE_DIR
"""Compare actual native discovery/check processes with the unchanged hardware source."""

from __future__ import annotations
import argparse
from dataclasses import asdict
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import tempfile
import time
from types import SimpleNamespace
from usbgpu_reference import ROOT, hardware_source, make_sysfs, source_power


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('binary_dir', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  source = hardware_source()
  records = []
  process = subprocess.Popen([args.binary_dir / 'examples/usbgpu_trace'], text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE)

  def compare(request: dict, expected) -> None:
    process.stdin.write(json.dumps(request) + '\n')
    process.stdin.flush()
    actual = json.loads(process.stdout.readline())
    records.append({'request': request, 'source': expected, 'native': actual, 'pass': actual == expected})
    assert actual == expected, records[-1]

  try:
    for config in [{'count': 0}, {'count': 2}, {}, {'speed': '480'}, {'speed': '5000.0'}, {'product': 'custom old-CLEAN'}]:
      with tempfile.TemporaryDirectory(prefix='usbgpu154-') as temporary:
        devices = make_sysfs(Path(temporary), **config)
        compare({'op': 'devices', 'path': str(devices)}, [asdict(device) for device in source.get_usbgpu_devices(devices)])
        for flags in itertools.product([False, True], repeat=5):
          state = dict(zip(['compiled', 'loading', 'active', 'startup_failed', 'compile_pending'], flags, strict=True))
          compare({'op': 'status', 'path': str(devices), 'state': state}, source.usbgpu_status(**state, devices_path=devices))
          compare({'op': 'badge', 'state': state}, source.usbgpu_badge_state(**state))
    with tempfile.TemporaryDirectory(prefix='usbgpu154-') as temporary:
      devices = make_sysfs(Path(temporary))
      for raw in [
        None,
        [1, 2, 3, 4],
        *[
          list(struct.pack('<HhB', voltage, current, fault))
          for voltage, current, fault in itertools.product([0, 7999, 8000, 12000, 65535], [-32768, -1, 0, 32767], [0, 1, 255])
        ],
      ]:
        expected, calls = source_power(source, devices, raw)
        compare({'op': 'power', 'bytes': raw}, expected)
        records[-1]['source_usb_calls'] = calls
  finally:
    process.stdin.close()
    assert process.wait(timeout=5) == 0
    (args.output / 'hardware.json').write_text(json.dumps(records, indent=2) + '\n')
  assert shutil.disk_usage(ROOT).free >= 26 * 1024**3, '25GiB reserve plus bounded fixture build headroom required'
  probe = args.output.resolve() / 'check-probe'
  command = [
    'cc',
    '-std=c11',
    '-D_POSIX_C_SOURCE=200809L',
    '-Wall',
    '-Wextra',
    '-Werror',
    str(ROOT / 'rust/crates/usbgpu/tests/native/check_probe.c'),
    '-o',
    str(probe),
  ]
  if shutil.disk_usage(args.output).free < 26 * 1024**3:
    raise RuntimeError("fixture build requires 25 GiB free plus 1 GiB estimated headroom")
  built = subprocess.run(command, capture_output=True, text=True)
  (args.output / 'fixture-build.json').write_text(
    json.dumps({'command': command, 'returncode': built.returncode, 'stdout': built.stdout, 'stderr': built.stderr}, indent=2) + '\n'
  )
  assert built.returncode == 0, built.stderr
  checks = []
  original_run = subprocess.run
  for mode, clean in [
    ('ok', True),
    ('pcie_once', True),
    ('pcie', True),
    ('read', True),
    ('bad', True),
    ('power', True),
    ('link', True),
    ('link', False),
    ('gone', True),
    ('sleep', True),
  ]:
    timeout = 0.15 if mode == 'sleep' else 15.0
    values = []
    for kind in ['source', 'native']:
      with tempfile.TemporaryDirectory(prefix='usbgpu154-') as temporary:
        root = Path(temporary)
        devices = make_sysfs(root)
        (root / 'mode').write_text(mode)
        environment = dict(os.environ, CHECK_FIXTURE_ROOT=str(root))
        calls = []

        def fixture_run(command, calls=calls, root=root, **kwargs):
          calls.append({'source_command': command, 'timeout': kwargs['timeout'], 'DEV': kwargs['env']['DEV'], 'GMMU': kwargs['env']['GMMU']})
          kwargs['env']['CHECK_FIXTURE_ROOT'] = str(root)
          return original_run([str(probe)], **kwargs)

        started = time.monotonic()
        if kind == 'source':
          source.subprocess = SimpleNamespace(run=fixture_run, TimeoutExpired=subprocess.TimeoutExpired)
          value = {'error': source.check_usbgpu(devices_path=devices, timeout=timeout, require_clean_link=clean)}
        else:
          invocation = [str(args.binary_dir / 'examples/usbgpu_check_fixture'), str(devices), str(probe), str(timeout), '1' if clean else '0']
          run = original_run(invocation, env=environment, capture_output=True, text=True, timeout=2 * timeout + 3)
          assert run.returncode == 0, run.stderr
          value = json.loads(run.stdout)
        child = int((root / 'pid').read_text())
        assert not Path(f'/proc/{child}').exists(), 'probe was not reaped'
        values.append(
          {
            'kind': kind,
            'result': value,
            'attempts': int((root / 'count').read_text()),
            'child_reaped': True,
            'seconds': time.monotonic() - started,
            'source_invocations': calls,
          }
        )
    checks.append(
      {'mode': mode, 'clean': clean, 'runs': values, 'pass': values[0]['result'] == values[1]['result'] and values[0]['attempts'] == values[1]['attempts']}
    )
    (args.output / 'process-checks.json').write_text(json.dumps(checks, indent=2) + '\n')
    assert checks[-1]['pass'], checks[-1]
  with tempfile.TemporaryDirectory(prefix='usbgpu154-') as temporary:
    root = Path(temporary)
    devices = make_sysfs(root)
    (root / 'mode').write_text('sleep')
    invocation = [str(args.binary_dir / 'examples/usbgpu_check_fixture'), str(devices), str(probe), '15', '1']
    process = subprocess.Popen(invocation, env=dict(os.environ, CHECK_FIXTURE_ROOT=str(root)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    deadline = time.monotonic() + 3
    while not (root / 'pid').exists():
      assert time.monotonic() < deadline
      time.sleep(0.01)
    child = int((root / 'pid').read_text())
    process.send_signal(signal.SIGTERM)
    stdout, stderr = process.communicate(timeout=3)
    assert process.returncode != 0 and not Path(f'/proc/{child}').exists()
    missing = original_run(
      [str(args.binary_dir / 'examples/usbgpu_check_fixture'), str(devices), str(root / 'missing'), '15', '1'], capture_output=True, text=True
    )
    assert missing.returncode != 0 and not missing.stdout
    (args.output / 'cancellation.json').write_text(
      json.dumps(
        {
          'pass': True,
          'command': invocation,
          'returncode': process.returncode,
          'stdout': stdout,
          'stderr': stderr,
          'child_reaped': True,
          'missing_probe_returncode': missing.returncode,
          'missing_probe_stderr': missing.stderr,
        },
        indent=2,
      )
      + '\n'
    )
  print(
    f'PASS {len(records)} unchanged-source discovery/status/power cases, {len(checks)} real probe-process comparisons, '
    + 'cancellation/reaping and distinct spawn failure'
  )


if __name__ == '__main__':
  main()
