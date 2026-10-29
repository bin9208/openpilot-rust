#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def run(command, environment, path):
  result = subprocess.run(command, env=environment, capture_output=True, text=True)
  path.write_text('COMMAND ' + repr(command) + '\n' + result.stdout + result.stderr + f'\nEXIT {result.returncode}\n')
  result.check_returncode()
  return result.stdout


def disk(path):
  free = shutil.disk_usage(path).free
  if free < 2 * 1024**3:
    raise RuntimeError('2 GiB estimated native build growth unavailable')
  return free


def passed_tests(output):
  summaries = re.findall(
    r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; \d+ ignored; \d+ measured; \d+ filtered out; finished in \d+(?:\.\d+)?s$',
    output, re.MULTILINE,
  )
  if len(summaries) != 1 or summaries[0][0] != 'ok' or int(summaries[0][1]) == 0 or int(summaries[0][2]) != 0:
    raise ValueError('missing, failed or empty ownership test summary')
  return int(summaries[0][1])


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  evidence = args.evidence.resolve()
  evidence.mkdir(parents=True, exist_ok=True)
  native = Path(__file__).resolve().parents[1] / 'crates/locationd/native'
  compiler = os.environ.get('CXX', 'clang++')
  libraries = [
    subprocess.check_output([compiler, '-print-file-name=' + name], text=True).strip()
    for name in ('libclang_rt.asan-x86_64.so', 'libclang_rt.ubsan_standalone-x86_64.so')
  ]
  assert all(Path(library).is_file() for library in libraries)
  environment = os.environ | {'LOCATIOND_SANITIZER_LIBS': os.pathsep.join(libraries), 'CARGO_INCREMENTAL': '0', 'CXX': compiler}
  free = [disk(evidence)]
  output = run(
    [
      'cargo',
      'test',
      '--manifest-path',
      'rust/Cargo.toml',
      '-p',
      'openpilot-locationd',
      '--no-default-features',
      '--features',
      'solver',
      '--test',
      'ownership',
      '--no-run',
      '--locked',
      '--message-format=json',
    ],
    environment,
    evidence / 'ownership-build.log',
  )
  binaries = [
    value['executable']
    for line in output.splitlines()
    if line.startswith('{')
    for value in [json.loads(line)]
    if value.get('reason') == 'compiler-artifact' and value['target']['name'] == 'ownership' and value.get('executable')
  ]
  assert len(binaries) == 1
  builds = [
    value['out_dir']
    for line in output.splitlines()
    if line.startswith('{')
    for value in [json.loads(line)]
    if value.get('reason') == 'build-script-executed' and 'openpilot-locationd' in value['package_id']
  ]
  assert len(builds) == 1
  symbols = run(['nm', '-u', str(Path(builds[0]) / 'liblocationd-rednose.a')], os.environ, evidence / 'instrumentation-symbols.log')
  assert '__asan_report' in symbols and '__ubsan_handle' in symbols
  sanitizer_env = os.environ | {
    'LD_PRELOAD': ' '.join(libraries),
    'ASAN_OPTIONS': 'detect_leaks=1:halt_on_error=1',
    'UBSAN_OPTIONS': 'halt_on_error=1:print_stacktrace=1',
  }
  ownership_output = run([binaries[0], '--nocapture'], sanitizer_env, evidence / 'ownership-sanitizer.log')
  ownership_tests = passed_tests(ownership_output)
  free.append(disk(evidence))
  scheduler = evidence / 'scheduler-sanitizer'
  run(
    [
      compiler,
      '-std=c++17',
      '-fsanitize=address,undefined',
      '-fno-omit-frame-pointer',
      '-g',
      str(native / 'scheduler.cc'),
      str(native / 'scheduler_fixture.cc'),
      '-o',
      str(scheduler),
    ],
    os.environ,
    evidence / 'scheduler-build.log',
  )
  run([str(scheduler)], os.environ, evidence / 'scheduler-sanitizer.log')
  result = {
    'pass': True,
    'production_filter_ownership_and_callbacks_instrumented': True,
    'ownership_tests': ownership_tests,
    'production_scheduler_instrumented': True,
    'free_bytes_before_builds': free,
    'test_executable': binaries[0],
  }
  (evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
