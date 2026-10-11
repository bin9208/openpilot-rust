#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# python3 rust/tools/cluster_usb_transport_compare.py ROOT ORACLE LIBRARY NATIVE OUTPUT
"""Compare display operations and bytes at an owned libusb recipient."""

from __future__ import annotations

import hashlib
import itertools
import json
import math
import os
from pathlib import Path
import subprocess
import sys
from typing import TypedDict


class Receipt(TypedDict):
  argv: list[str]
  returncode: int
  stdout: str
  stderr: str


def cases() -> list[tuple[str, str, dict]]:
  frame = [{'op': 'open'}, {'op': 'jpeg', 'bytes': list(range(251))}, {'op': 'png', 'bytes': []}, {'op': 'close'}]
  rows = []
  for fast, ack in itertools.product((False, True), repeat=2):
    rows.append((f'frame-{fast}-{ack}', 'ok', {'options': {'fast_write': fast, 'wait_for_frame_ack': ack}, 'actions': frame}))
  rows.append(('pid123', 'pid123', {'options': {'expected_product_id': 0x123}, 'actions': frame}))
  settings = [
    {'op': 'brightness', 'value': 24, 'force': False},
    {'op': 'orientation', 'value': 2, 'force': False},
    {'op': 'fps', 'value': 0, 'force': False},
    {'op': 'open'},
  ]
  for op, values in [('brightness', [-3, 24, 100, 104]), ('orientation', [1, 2, 0]), ('fps', [0, 20, 999])]:
    for value in values:
      for force in [False, True]:
        settings.append({'op': op, 'value': value, 'force': force})
  settings.append({'op': 'close'})
  rows.append(('settings', 'ok', {'actions': settings}))
  for mode in ['missing', 'open_error', 'config_error', 'no_interface0']:
    rows.append((mode, mode, {'actions': frame}))
  for mode in ['write_error', 'disconnect', 'short_write', 'empty_ack', 'ack_timeout']:
    rows.append((mode, mode, {'options': {'wait_for_frame_ack': True}, 'actions': [frame[0], frame[1], frame[1], frame[1], frame[-1]]}))
  for mode in ['clear_error', 'input_disconnect', 'write_io', 'intermittent']:
    rows.append((mode, mode, {'options': {'wait_for_frame_ack': True}, 'actions': [frame[0], *[frame[1]] * 5, frame[-1]]}))
  rows.append(('loading-frame-recovery', 'write_error', {'reset_protected': True, 'actions': frame}))
  for fast in [False, True]:
    rows.append(
      (
        f'bounded-drains-{fast}',
        'drain_success',
        {
          'options': {
            'fast_write': fast,
            'frame_drain_attempts': 4,
            'frame_drain_timeout_ms': 3,
            'fast_frame_drain_attempts': 7,
            'fast_frame_drain_timeout_ms': 5,
          },
          'actions': frame,
        },
      )
    )
  for protected in [False, True]:
    rows.append((f'init-recovery-{protected}', 'sync_timeout_once', {'reset_protected': protected, 'actions': frame}))
  for gpu, requested in itertools.product((False, True), (0, 1, 1_100_000)):
    actions = [{'op': 'open'}, {'op': 'start_h264', 'requested': requested}]
    for wait, require in itertools.product((False, True), repeat=2):
      actions.append({'op': 'chunk', 'bytes': [0, 0, 1, 65, 12, 31], 'last': require, 'wait': wait, 'require': require})
    actions.extend([{'op': 'chunk', 'bytes': [], 'last': True, 'wait': True, 'require': True}, {'op': 'stop_h264'}, {'op': 'close'}])
    rows.append((f'h264-{gpu}-{requested}', 'ok', {'gpu_active': gpu, 'reset_protected': gpu, 'actions': actions}))
  for mode in ['invalid_chunk', 'busy_status', 'ack_timeout', 'disconnect']:
    rows.append(
      (
        f'h264-{mode}',
        mode,
        {
          'actions': [
            {'op': 'open'},
            {'op': 'start_h264', 'requested': 0},
            {'op': 'chunk', 'bytes': [0, 0, 1, 65], 'last': False, 'wait': True, 'require': False},
            {'op': 'close'},
          ]
        },
      )
    )
  return rows


def invoke(argv: list[str], environment: dict[str, str], directory: Path, stdin: str | None) -> Receipt:
  directory.mkdir(parents=True)
  (directory / 'wire.txt').write_bytes(b'')
  (directory / 'bytes.bin').write_bytes(b'')
  environment = {
    **environment,
    'USB_FIXTURE_LOG': str(directory / 'libusb.log'),
    'USB_FIXTURE_WIRE': str(directory / 'wire.txt'),
    'USB_FIXTURE_BYTES': str(directory / 'bytes.bin'),
  }
  result = subprocess.run(argv, input=stdin, env=environment, capture_output=True, text=True, timeout=20, check=False)
  receipt: Receipt = {'argv': argv, 'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}
  (directory / 'invocation.json').write_text(
    json.dumps(
      {
        **receipt,
        'environment': {key: value for key, value in environment.items() if key.startswith('USB_FIXTURE') or key in {'LD_LIBRARY_PATH', 'PYTHONPATH'}},
      },
      indent=2,
    )
    + '\n'
  )
  assert result.returncode == 0, receipt
  assert (directory / 'libusb.log').read_text().endswith('exit clean\n'), directory
  return receipt


def compare(root: Path, oracle: Path, library: Path, native: Path, output: Path, profile: bool = False) -> None:
  output.mkdir(parents=True)
  environment = {**os.environ, 'LD_LIBRARY_PATH': str(library.parent), 'PYTHONNOUSERSITE': '1'}
  rows = []
  scenarios = cases()
  if profile:
    for _, _, case in scenarios:
      case['profile_enabled'] = True
    scenarios.extend(
      [
        ('profile-disabled', 'ok', {'profile_enabled': False, 'actions': [{'op': 'open'}, {'op': 'jpeg', 'bytes': [1]}, {'op': 'close'}]}),
        (
          'profile-clear-toggle',
          'ok',
          {
            'profile_enabled': True,
            'actions': [
              {'op': 'open'},
              {'op': 'jpeg', 'bytes': [1]},
              {'op': 'clear_profile'},
              {'op': 'profile_enabled', 'value': False},
              {'op': 'jpeg', 'bytes': [2]},
              {'op': 'profile_enabled', 'value': True},
              {'op': 'png', 'bytes': [3]},
              {'op': 'close'},
            ],
          },
        ),
      ]
    )
  for name, mode, case in scenarios:
    directory = output / name
    directory.mkdir()
    case['lock'] = str(directory / 'bus.lock')
    data = json.dumps(case)
    (directory / 'input.json').write_text(data + '\n')
    env = {**environment, 'USB_FIXTURE_MODE': mode}
    source = invoke(
      [sys.executable, str(root / 'rust/tools/cluster_usb_transport_source.py'), str(root), str(oracle), str(library), str(directory / 'input.json')],
      env,
      directory / 'source',
      None,
    )
    actual = invoke([str(native)], env, directory / 'native', data)
    expected = json.loads(source['stdout'].splitlines()[-1])
    observed = json.loads(actual['stdout'].splitlines()[-1])

    def normalized(values: list[dict]) -> list[dict]:
      return [{'Err': True} if 'Err' in value else value for value in values]

    result = {
      'source': expected,
      'native': observed,
      'results_match': normalized(expected['results']) == normalized(observed['results']),
      'runtime_match': expected['runtime'] == observed['runtime'],
      'dimensions_match': expected['dimensions'] == observed['dimensions'],
      'wire_match': (directory / 'source/wire.txt').read_bytes() == (directory / 'native/wire.txt').read_bytes(),
      'bytes_match': (directory / 'source/bytes.bin').read_bytes() == (directory / 'native/bytes.bin').read_bytes(),
    }
    if profile:
      result['profile_match'] = [name for name, _ in expected['profiles']] == [name for name, _ in observed['profiles']]
      result['profile_durations_valid'] = all(math.isfinite(value) and value >= 0 for report in [expected, observed] for _, value in report['profiles'])
    result['pass'] = all(result[key] for key in ['results_match', 'runtime_match', 'dimensions_match', 'wire_match', 'bytes_match'])
    if profile:
      result['pass'] = result['pass'] and result['profile_match'] and result['profile_durations_valid']
    (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    rows.append({'scenario': name, 'pass': result['pass']})
  result = {
    'pass': all(row['pass'] for row in rows),
    'rows': rows,
    'binary_sha256': hashlib.sha256(native.read_bytes()).hexdigest(),
    'scope': 'unchanged original display methods and actual PyUSB versus Rust through owned libusb ABI; no physical display/Params/runtime claim',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['pass'], rows


if __name__ == '__main__':
  profile = sys.argv[1] == '--profile'
  compare(*(Path(value).resolve() for value in sys.argv[2 if profile else 1 :]), profile=profile)
