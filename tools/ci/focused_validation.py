#!/usr/bin/env python3
"""Bounded host checks and recorded, exact-commit device acceptance (#685)."""
import argparse
import ast
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[2]
HOST_ONLY = {
    'AGENTS.md', 'tools/ci/focused_validation.py',
    'tools/native/publish_inputs.py',
    'rust/crates/dev-tools/src/publish.rs',
    'rust/crates/dev-tools/tests/publish.rs',
}
HOST_PREFIXES = ('.github/', 'docs/', 'tools/ci/tests/')
RUST_PACKAGES = {
    'runtime-core': 'openpilot-runtime-core',
    'runtime-build': 'openpilot-runtime-build',
    'runtime-installer': 'openpilot-runtime-installer',
    'disk-space': 'openpilot-disk-space',
}


def changed_paths(base, head='HEAD'):
  # A rename includes its old and new path, so moving runtime code under docs
  # cannot turn a device-affecting change into a documentation-only exception.
  raw = subprocess.check_output(['git', 'diff', '--no-renames', '--name-only',
                                 '-z', base, head], cwd=ROOT)
  return [os.fsdecode(path) for path in raw.split(b'\0') if path]


def field(body, name):
  values = re.findall(r'^' + re.escape(name) + r':\s*([^\n\r]+)$', body, re.MULTILINE)
  return values[0].strip() if len(values) == 1 else ''


def require_device(paths, pull_request):
  body = pull_request.get('body') or ''
  if paths and all(path in HOST_ONLY or path.startswith(HOST_PREFIXES) for path in paths):
    reason = field(body, 'Device-Test-Not-Needed')
    if len(reason) < 12:
      raise ValueError('문서/CI 변경의 실기 생략 사유를 Device-Test-Not-Needed에 기록하세요.')
    return '실기 반복 생략: ' + reason
  expected = pull_request['head']['sha']
  if field(body, 'Device-Tested-SHA') != expected:
    raise ValueError('수정 브랜치의 현재 40자리 HEAD SHA로 실기 빌드·부팅 확인이 필요합니다.')
  for name in ('Device-Build', 'Device-Boot', 'Device-Affected-Behavior'):
    if field(body, name) != 'passed':
      raise ValueError(name + ': passed는 사용자의 실제 확인 후에만 기록하세요.')
  if len(field(body, 'Device-Environment')) < 8:
    raise ValueError('실제로 사용한 기기/OS 환경을 Device-Environment에 기록하세요.')
  evidence = field(body, 'Device-Test-Evidence')
  if not re.fullmatch(r'https://github\.com/bin9208/openpilot-rust/(?:issues|pull)/\d+(?:#[\w-]+)?', evidence):
    raise ValueError('사용자 실기 결과의 이슈/PR 링크를 Device-Test-Evidence에 기록하세요.')
  return '현재 커밋의 사용자 실기 수용 기록: ' + evidence


def selected_tests(paths):
  suites = {'test_focused_validation.py'}
  for path in paths:
    name = Path(path).stem
    if path.startswith('tools/ci/tests/test_') and path.endswith('.py'):
      suites.add(Path(path).name)
    if path.startswith('tools/native/'):
      if name in ('source_entry', 'source_runtime', 'build_memory', 'build_storage'):
        suites.update(('test_native_source_entry.py', 'test_native_source_runtime.py'))
      elif name == 'bootstrap_seed':
        suites.add('test_native_bootstrap_seed.py')
      elif name == 'publish_inputs':
        suites.add('test_native_input_publication.py')
    candidate = ROOT / 'tools/ci/tests' / ('test_' + name + '.py')
    if path.startswith('tools/ci/') and candidate.is_file():
      suites.add(candidate.name)
  return sorted(suites)


def run(command, timeout=180):
  print('+ ' + ' '.join(command), flush=True)
  subprocess.run(command, cwd=ROOT, check=True, timeout=timeout)


def host_checks(paths):
  for name in paths:
    path = ROOT / name
    if not path.is_file() or path.is_symlink():
      continue
    if path.suffix == '.py':
      ast.parse(path.read_bytes(), filename=name)
    elif path.suffix == '.json':
      json.loads(path.read_bytes())
    elif path.suffix == '.toml':
      tomllib.loads(path.read_text())
    elif path.suffix in ('.yml', '.yaml'):
      import yaml
      yaml.safe_load(path.read_text())
    elif path.suffix == '.sh':
      run(['bash', '-n', name])
    elif path.suffix == '.rs':
      run(['rustfmt', '--edition', '2021', '--check', '--config', 'skip_children=true', name])
  for suite in selected_tests(paths):
    run([sys.executable, '-m', 'pytest', '-q', '-p', 'no:cacheprovider', '-c', '/dev/null', '--rootdir=.',
         '--confcutdir=tools/ci/tests', 'tools/ci/tests/' + suite])
  # Always retain one small, real x86 Rust test. Only bounded known host crates
  # are added automatically; native providers and the workspace run on device
  # or through an explicitly requested diagnostic workflow.
  crates = {'openpilot-runtime-core'}
  for name in paths:
    parts = name.split('/')
    if len(parts) > 3 and parts[:2] == ['rust', 'crates'] and parts[2] in RUST_PACKAGES:
      crates.add(RUST_PACKAGES[parts[2]])
  for crate in sorted(crates):
    run(['cargo', 'test', '--manifest-path', 'rust/Cargo.toml', '--locked',
         '-p', crate, '--lib', '--tests', '--no-default-features'], timeout=300)
  if any(name in HOST_ONLY and name.endswith('publish.rs') for name in paths):
    run(['cargo', 'test', '--manifest-path', 'rust/Cargo.toml', '--locked',
         '-p', 'openpilot-dev-tools', '--test', 'publish'], timeout=300)
  print('집중 검사 완료. 전체 workspace/ARM/AGNOS 검증과 실기 실행은 수행하지 않았습니다.')


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('mode', choices=('host', 'device'))
  parser.add_argument('--base', required=True)
  args = parser.parse_args()
  paths = changed_paths(args.base)
  event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
  if (os.environ.get('GITHUB_EVENT_NAME') == 'pull_request'
      and event['pull_request']['base']['ref'] == 'dev'):
    md = subprocess.run(['git', 'merge-base', '--is-ancestor', 'origin/dev-md', 'HEAD'],
                        cwd=ROOT, capture_output=True)
    if md.returncode == 0:
      raise ValueError('dev-md runtime을 dev에 통합할 수 없습니다 (#338).')
    if md.returncode not in (0, 1):
      raise ValueError('dev-md 격리 기준을 확인하지 못했습니다.')
  if args.mode == 'host':
    run(['git', 'diff', '--check', args.base, 'HEAD'])
    host_checks(paths)
  else:
    if os.environ.get('GITHUB_EVENT_NAME') != 'pull_request':
      # Only protected integration branches receive push runs. There is no
      # dispatch entry point that could green an untested issue branch.
      if os.environ.get('GITHUB_REF_NAME') not in ('dev', 'dev-md', 'main'):
        raise ValueError('실기 수용은 PR의 현재 커밋에서 확인해야 합니다.')
      print('병합 후 검사: 실기 수용 근거는 보호된 병합 PR의 기록을 참조합니다.')
    else:
      print(require_device(paths, event['pull_request']))


if __name__ == '__main__':
  main()
