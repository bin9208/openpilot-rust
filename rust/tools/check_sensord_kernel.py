#!/usr/bin/env python3
"""Real source/native Linux UAPI calls intercepted only on owned regular files, plus ASan/UBSan."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  root = Path(__file__).resolve().parents[2]
  fixture = root / 'rust/tools/sensord_kernel_fixture.c'
  kernel = root / 'rust/crates/sensord/native'
  shared = args.evidence / 'sensord-kernel-fixture.so'
  with (args.evidence / 'kernel-build.log').open('w') as build:
    subprocess.run(
      ['clang', '-shared', '-fPIC', '-std=gnu11', '-O1', '-Wall', '-Wextra', str(fixture), '-ldl', '-pthread', '-o', shared],
      stdout=build,
      stderr=subprocess.STDOUT,
      check=True,
    )
    obj = args.evidence / 'sensord-kernel-fixture-asan.o'
    subprocess.run(
      ['clang', '-fPIC', '-std=gnu11', '-O1', '-g', '-fsanitize=address,undefined', '-fno-omit-frame-pointer', '-c', fixture, '-o', obj],
      stdout=build,
      stderr=subprocess.STDOUT,
      check=True,
    )
    peer = args.evidence / 'sensord-kernel-asan'
    subprocess.run(
      [
        'clang++',
        '-std=c++17',
        '-O1',
        '-g',
        '-fsanitize=address,undefined',
        '-fno-omit-frame-pointer',
        '-I',
        kernel,
        kernel / 'kernel.cc',
        kernel / 'sanitizer_peer.cc',
        obj,
        '-ldl',
        '-pthread',
        '-o',
        peer,
      ],
      stdout=build,
      stderr=subprocess.STDOUT,
      check=True,
    )
    build.write('PASS fixture and production-kernel sanitizer peer built\n')
  results = []
  with tempfile.TemporaryDirectory(prefix='sensord-kernel-') as temporary:
    folder = Path(temporary)
    device, gpio = folder / 'i2c', folder / 'gpio'
    device.touch()
    gpio.touch()
    for name, command in [('source', [sys.executable, str(Path(__file__).with_name('sensord_linux_source.py'))]), ('native', [str(args.binary.resolve())])]:
      trace, state = args.evidence / f'kernel-{name}.jsonl', args.evidence / f'kernel-{name}-state.json'
      trace.unlink(missing_ok=True)
      environment = os.environ | {
        'LD_PRELOAD': str(shared.resolve()),
        'SENSORD_I2C': str(device),
        'SENSORD_GPIO': str(gpio),
        'SENSORD_TRACE': str(trace.resolve()),
        'SENSORD_STATE': str(state.resolve()),
        'PYTHONPATH': str(root),
      }
      result = subprocess.run([*command, device, gpio], capture_output=True, text=True, env=environment, timeout=8)
      (args.evidence / f'kernel-{name}.stderr').write_text(result.stderr or '(no stderr)\n')
      assert result.returncode == 0, result.stderr
      captured = {
        'rows': json.loads(result.stdout),
        'calls': [json.loads(line) for line in trace.read_text().splitlines()],
        'state': json.loads(state.read_text()),
      }
      (args.evidence / f'kernel-{name}.json').write_text(json.dumps(captured, indent=2) + '\n')
      results.append(captured)
    assert results[0] == results[1], 'source/native kernel boundary mismatch'
    trace, state = args.evidence / 'kernel-asan.jsonl', args.evidence / 'kernel-asan-state.json'
    trace.unlink(missing_ok=True)
    environment = os.environ | {
      'SENSORD_I2C': str(device),
      'SENSORD_GPIO': str(gpio),
      'SENSORD_TRACE': str(trace.resolve()),
      'SENSORD_STATE': str(state.resolve()),
      'ASAN_OPTIONS': 'detect_leaks=1:halt_on_error=1',
      'UBSAN_OPTIONS': 'halt_on_error=1',
    }
    result = subprocess.run([peer.resolve(), device, gpio], capture_output=True, text=True, env=environment, timeout=10)
    (args.evidence / 'kernel-sanitizers.log').write_text(result.stdout + result.stderr + f'\nexit_code: {result.returncode}\n')
    assert result.returncode == 0 and 'PASS native SMBus/GPIO/scheduling boundary' in result.stdout, result.stderr
  report = {
    'source_native_equal': True,
    'asan_ubsan_passed': True,
    'owned_regular_files_only': True,
    'real_scheduling_changed': False,
    'artifact': 'kernel-native.json',
  }
  (args.evidence / 'kernel-results.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
