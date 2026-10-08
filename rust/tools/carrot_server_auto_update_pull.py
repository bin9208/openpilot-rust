#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# uv run rust/tools/carrot_server_auto_update_pull.py --output OUTPUT [--binary BINARY] [--case NAME]
# ──────────────────
"""Run the unchanged pinned-update transaction in independently owned repositories."""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
from typing import TypeAlias, TypedDict, assert_never

from carrot_server_auto_update_pull_cases import CASES, Case, Mode, Snapshot, assert_repository, prepare, snapshot, template

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'rust/tools'

Json: TypeAlias = str | bool | int | float | None | list['Json'] | dict[str, 'Json']


class Child(TypedDict):
  kind: str
  argv: list[str]
  cwd: str
  pid: int
  pgid: int
  starttime: str
  inherited_lock: bool
  contender_blocked: bool


class Run(TypedDict):
  config: dict[str, Json]
  command: list[str]
  elapsed_seconds: float
  returncode: int
  stdout: str
  stderr: str
  result: dict[str, Json]
  children: list[Child]
  child_checks: dict[str, bool]
  repository_after: Snapshot
  state_bytes_hex: str | None


class Observable(TypedDict):
  result: dict[str, Json]
  repository_after: Snapshot
  state_bytes_hex: str | None
  console: list[str]


def lock_released(path: Path) -> bool:
  with path.open('r+b') as descriptor:
    try:
      fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
      return False
  return True


def alive(row: Child) -> bool:
  path = Path(f"/proc/{row['pid']}/stat")
  try:
    fields = path.read_text().rsplit(')', 1)[1].split()
  except FileNotFoundError:
    return False
  return fields[19] == row['starttime'] and fields[0] != 'Z'


def cleanup_groups(log: Path) -> None:
  rows: list[Child] = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
  owned = {row['pgid'] for row in rows if row['kind'] == 'git' and row['pgid'] == row['pid']}
  for row in rows:
    if row['pgid'] in owned and alive(row):
      try:
        if os.getpgid(row['pid']) == row['pgid'] and row['pgid'] != os.getpgrp():
          os.killpg(row['pgid'], signal.SIGKILL)
      except ProcessLookupError:
        continue


def execute(case: Case, directory: Path, base: Path, heads: dict[str, str], environment: dict[str, str], binary: Path | None) -> Run:
  directory.mkdir(parents=True)
  repository, state, target = prepare(case, directory, base, heads, environment)
  wrappers = directory / 'bin'
  wrappers.mkdir()
  wrapper = TOOLS / 'carrot_server_auto_update_pull_git.py'
  wrapper.chmod(0o755)
  if case.mode == Mode.RESET_EXEC_DENIED:
    shutil.copy2(wrapper, wrappers / 'git')
  else:
    (wrappers / 'git').symlink_to(wrapper)
  lock = directory / 'repository.lock'
  child_log = directory / 'git.jsonl'
  child_environment = environment | {
    'PATH': str(wrappers) + ':' + environment['PATH'],
    'PYTHONPATH': str(TOOLS) + ':' + str(ROOT),
    'CARROT_REPO_LOCK_PATH': str(lock),
    'OWNED_PULL_REPOSITORY': str(repository),
    'OWNED_PULL_STATE': str(state),
    'OWNED_PULL_LOG': str(child_log),
    'OWNED_PULL_COUNTER': str(directory / 'head-count'),
    'OWNED_PULL_MODE': case.mode,
    'OWNED_PULL_OTHER_HEAD': heads['other'],
    'OWNED_PULL_READY': str(directory / 'cancel-ready'),
    'OWNED_PULL_WRITE_READY': '0' if case.mode == Mode.CANCEL_NO_READY else '1',
  }
  if case.mode == Mode.RESET_EXEC_DENIED:
    (wrappers / 'python3').symlink_to(sys.executable)
    child_environment['PATH'] = str(wrappers)
  config = {
    'repository': str(repository), 'state': str(state), 'lock': str(lock),
    'target': target, 'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'),
    'cancel_after_ms': 10 if case.mode in (Mode.CANCEL_GROUP, Mode.CANCEL_NO_READY) else None,
    'cancel_ready': str(directory / 'cancel-ready') if case.mode in (Mode.CANCEL_GROUP, Mode.CANCEL_NO_READY) else None,
    'alert_failure': case.alert_failure, 'notify_failure': case.notify_failure,
  }
  command = [str(binary)] if binary else [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_pull_source.py')]
  assert_repository(repository)
  started = time.monotonic()
  try:
    run = subprocess.run(command, input=json.dumps(config), cwd=ROOT, env=child_environment, capture_output=True, text=True, timeout=20)
  except subprocess.TimeoutExpired:
    cleanup_groups(child_log)
    raise
  result = json.loads(run.stdout.splitlines()[-1]) if run.returncode == 0 else {'failed_exit': run.returncode}
  children = [json.loads(line) for line in child_log.read_text().splitlines()] if child_log.exists() else []
  child_checks = {
    'inherited_lock': all(row['inherited_lock'] for row in children),
    'contender_blocked': all(row['contender_blocked'] for row in children),
    'no_owned_child_alive': not any(alive(row) for row in children),
    'lock_released': lock_released(lock),
  }
  if case.mode in (Mode.CANCEL_GROUP, Mode.CANCEL_NO_READY):
    child_checks['descendant_observed'] = any(row['kind'] == 'descendant' for row in children)
  row = {
    'config': config, 'command': command, 'elapsed_seconds': time.monotonic() - started,
    'returncode': run.returncode, 'stdout': run.stdout, 'stderr': run.stderr,
    'result': result, 'children': children, 'child_checks': child_checks,
    'repository_after': snapshot(repository),
    'state_bytes_hex': (state / 'git.json').read_bytes().hex() if state.is_dir() and (state / 'git.json').is_file() else None,
  }
  (directory / 'result.json').write_text(json.dumps(row, ensure_ascii=True, indent=2) + '\n')
  (directory / 'stdout.log').write_text(run.stdout)
  (directory / 'stderr.log').write_text(run.stderr)
  return row


def expected(case: Case, row: dict, heads: dict[str, str]) -> bool:
  assert row['returncode'] == 0 and all(row['child_checks'].values())
  result = row['result']
  assert all(result['effect_locks'])
  output = result['output']
  match case.mode:
    case Mode.RESET_BUSY | Mode.MERGE_BUSY:
      assert output['exception'] == 'RepoBusyError' and 'index.lock' in output['message'] and 'File exists' in output['message']
    case Mode.CANCEL_GROUP:
      assert output == {'exception': 'CancelledError', 'message': ''}
    case (Mode.NORMAL | Mode.RESET_OUTPUT | Mode.RESET_SIGNAL | Mode.POST_HEAD_FAIL | Mode.POST_HEAD_MISMATCH |
          Mode.UPDATED_STATE_FAIL | Mode.RESET_EXEC_DENIED):
      values = (True, True, heads['target']) if case.name in {
        'pinned-target-dirty-reset', 'alert-failure-keeps-verified-update', 'notification-failure-keeps-verified-update',
      } else (True, False, heads['base']) if case.target == 'base' else (
        True, False, heads['other']) if case.mode == 'post-head-mismatch' else (
        True, False, heads['target']) if case.mode == 'updated-state-fail' else (False, False, '')
      assert output['result'] == list(values)
    case unreachable:
      assert_never(unreachable)
  if case.mode == 'reset-output':
    assert result['state']['auto_update']['error'] == 'first � second 오류'
    assert result['state']['auto_update']['reset_rc'] == 23
  if case.mode == 'reset-signal':
    assert result['state']['auto_update']['reset_rc'] == -15
  if case.name == 'pinned-target-dirty-reset':
    assert row['repository_after']['head'] == heads['target']
    assert row['repository_after']['tracked'] == 'target\n'
    assert row['repository_after']['fetch_head'].startswith(heads['other'])
    assert result['state']['git_pull_time'] == 1700000000
    assert result['state']['git_pull_ok'] is True
    assert result['state']['auto_update']['status'] == 'updated'
  if case.blocked_state:
    assert row['repository_after']['tracked'] == 'owned dirty edit\n'
    assert [item['argv'] for item in row['children']] == [['rev-parse', 'HEAD']]
  if case.receipt:
    assert not row['children'] and result['state']['auto_update']['reboot_requested_head'] == heads['target']
  return True


def observable(row: Run) -> Observable:
  raw = json.dumps({'result': row['result'], 'repository_after': row['repository_after'],
                    'state_bytes_hex': row['state_bytes_hex'], 'console': row['stdout'].splitlines()[:-1]}, ensure_ascii=True)
  for key in ('repository', 'state'):
    raw = raw.replace(row['config'][key], '<owned-' + key + '>')
  return json.loads(raw)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--case')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  free = shutil.disk_usage(ROOT).free
  guard = {'free_bytes': free, 'floor_bytes': 25 * 1024**3, 'estimated_growth_bytes': 16 * 1024**2,
           'allowed': free >= 25 * 1024**3 + 16 * 1024**2}
  (args.output / 'guard.json').write_text(json.dumps(guard, indent=2) + '\n')
  assert guard['allowed']
  environment = os.environ | {'GIT_AUTHOR_DATE': '2026-09-30T12:00:00+00:00', 'GIT_COMMITTER_DATE': '2026-09-30T12:00:00+00:00',
                              'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null', 'LC_ALL': 'C.UTF-8'}
  base, heads = template(args.output, environment)
  rows = []
  for case in CASES:
    if args.case is not None and case.name != args.case:
      continue
    source = execute(case, args.output / case.name / 'source', base, heads, environment, None)
    expected(case, source, heads)
    native = execute(case, args.output / case.name / 'native', base, heads, environment, args.binary.resolve()) if args.binary else None
    equal = observable(source) == observable(native) if native else None
    commands_equal = ([item['argv'] for item in source['children']] == [item['argv'] for item in native['children']]) if native else None
    native_checks = all(native['child_checks'].values()) and native['returncode'] == 0 if native else None
    row = {'case': case.name, 'source': source, 'native': native, 'equal': equal,
           'commands_equal': commands_equal, 'native_child_checks': native_checks}
    rows.append(row)
    (args.output / 'result.json').write_text(json.dumps(rows, ensure_ascii=True, indent=2) + '\n')
    print(case.name, 'PASS' if native is None or (equal and commands_equal and native_checks) else 'FAIL', flush=True)
  assert rows, 'No selected scenarios'
  identity = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in [
    ROOT / 'openpilot/selfdrive/carrot/server/services/auto_update.py', ROOT / 'openpilot/common/async_process.py',
    ROOT / 'openpilot/common/repo_update.py', ROOT / 'openpilot/selfdrive/carrot/server/services/git_state.py',
    *(TOOLS / name for name in ['carrot_server_auto_update_pull.py', 'carrot_server_auto_update_pull_cases.py',
                               'carrot_server_auto_update_pull_git.py', 'carrot_server_auto_update_pull_source.py']),
    *([args.binary.resolve()] if args.binary else []),
  ]}
  (args.output / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
  raise SystemExit(0 if args.binary is None or all(row['equal'] and row['commands_equal'] and row['native_child_checks'] for row in rows) else 1)


if __name__ == '__main__':
  main()
