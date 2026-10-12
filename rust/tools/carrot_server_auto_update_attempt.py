#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run: uv run rust/tools/carrot_server_auto_update_attempt.py OUTPUT [--binary BINARY] [--case NAME]
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from typing import TypedDict

from carrot_server_auto_update_attempt_cases import CASES, Case, Config, Mode, Result
from carrot_server_auto_update_pull import Child, alive, cleanup_groups, lock_released
from carrot_server_auto_update_pull_cases import Snapshot, assert_repository, git, snapshot, template

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'rust/tools'


class Run(TypedDict):
  returncode: int
  command: list[str]
  config: Config
  elapsed_seconds: float
  stdout: str
  stderr: str
  result: Result
  children: list[Child]
  phases: dict[int, str]
  repository_after: Snapshot
  lock_released: bool
  no_owned_child_alive: bool


class Observable(TypedDict):
  result: Result
  repository_after: Snapshot
  commands: list[tuple[str, list[str]]]
  console: list[str]


def execute(case: Case, directory: Path, base: Path, heads: dict[str, str], binary: Path | None) -> Run:
  directory.mkdir(parents=True)
  repository, remote, state = directory / 'repository', directory / 'origin.git', directory / 'state'
  shutil.copytree(base, repository)
  subprocess.run(['/usr/bin/git', 'clone', '--bare', '-q', str(base), str(remote)], check=True)
  subprocess.run(['/usr/bin/git', '-C', str(remote), 'update-ref', 'refs/heads/owned-update', heads['base' if case.up_to_date else 'target']], check=True)
  git(repository, 'reset', '--hard', heads['base'])
  git(repository, 'branch', 'owned-other', heads['base'])
  git(repository, 'remote', 'add', 'origin', remote.as_uri())
  git(repository, 'config', 'branch.owned-update.remote', 'origin')
  git(repository, 'config', 'branch.owned-update.merge', 'refs/heads/owned-update')
  (repository / 'tracked.txt').write_text('owned dirty working file\n')
  state.mkdir()
  initial = {'unrelated': 'preserved'}
  if case.pulling:
    initial['auto_update'] = {'status': 'pulling', 'error': 'old-error'}
  (state / 'git.json').write_text(json.dumps(initial))
  wrappers, proc = directory / 'bin', directory / 'proc'
  wrappers.mkdir()
  proc.mkdir()
  wrapper = TOOLS / 'carrot_server_auto_update_attempt_git.py'
  wrapper.chmod(0o755)
  (wrappers / 'git').symlink_to(wrapper)
  lock, phase, log, phase_log = directory / 'repository.lock', directory / 'phase', directory / 'git.jsonl', directory / 'phases.txt'
  config: Config = {
    'repository': str(repository), 'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'),
    'state': str(state), 'lock': str(lock), 'proc_root': str(proc), 'phase': str(phase), 'base': heads['base'],
    'steps': [{'now': now, 'ready': list(case.ready), 'restore': index > 0, 'warm': case.warm} for index, now in enumerate(case.times)],
    'busy': case.busy, 'index_lock': case.index_lock, 'cancel': case.mode == Mode.CONFIG_CANCEL,
    'cancel_ready': str(directory / 'config-ready'), 'cancel_release': str(directory / 'config-release'),
  }
  environment = os.environ | {
    'PATH': str(wrappers) + ':' + os.environ['PATH'], 'PYTHONPATH': str(TOOLS) + ':' + str(ROOT),
    'CARROT_REPO_LOCK_PATH': str(lock), 'OWNED_PULL_REPOSITORY': str(repository),
    'OWNED_PULL_LOG': str(log), 'OWNED_PULL_OTHER_HEAD': heads['other'],
    'OWNED_ATTEMPT_MODE': case.mode, 'OWNED_ATTEMPT_PHASE': str(phase),
    'OWNED_ATTEMPT_PHASE_LOG': str(phase_log), 'OWNED_ATTEMPT_REMOTE': str(remote),
    'OWNED_ATTEMPT_CANCEL_READY': config['cancel_ready'], 'OWNED_ATTEMPT_CANCEL_RELEASE': config['cancel_release'],
  }
  command = [str(binary)] if binary else [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_attempt_source.py')]
  assert_repository(repository)
  started = time.monotonic()
  try:
    run = subprocess.run(command, input=json.dumps(config), env=environment, cwd=ROOT, capture_output=True, text=True, timeout=25)
  except subprocess.TimeoutExpired:
    cleanup_groups(log)
    raise
  children = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
  phases = {int(pid): value for pid, value in (line.split() for line in phase_log.read_text().splitlines())} if phase_log.exists() else {}
  row = Run(returncode=run.returncode, command=command, config=config, elapsed_seconds=time.monotonic() - started,
    stdout=run.stdout, stderr=run.stderr, result=json.loads(run.stdout.splitlines()[-1]) if run.returncode == 0 else {},
    children=children, phases=phases, repository_after=snapshot(repository), lock_released=lock_released(lock),
    no_owned_child_alive=not any(alive(child) for child in children))
  (directory / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
  (directory / 'stdout.log').write_text(run.stdout)
  (directory / 'stderr.log').write_text(run.stderr)
  return row


def observable(row: Run) -> Observable:
  path = Path(row['config']['repository']).parent
  after = row['repository_after'].copy()
  if after['fetch_head'] is not None:
    after['fetch_head'] = after['fetch_head'].replace(str(path), 'SIDE')
  return {
    'result': row['result'], 'repository_after': after,
    'commands': [(row['phases'][child['pid']], child['argv']) for child in row['children']],
    'console': [line.replace(str(path), 'SIDE') for line in row['stdout'].splitlines()[:-1]],
  }


def expected(case: Case, row: Run, heads: dict[str, str]) -> None:
  assert row['returncode'] == 0, row['stderr']
  assert row['lock_released'] and row['no_owned_child_alive']
  assert all(child['inherited_lock'] and child['contender_blocked'] for child in row['children'])
  result = row['result']
  steps = result['steps']
  assert all(effect.get('lock_held', effect.get('cancel_lock_held', False)) for effect in result['effects'])
  if case.mode == Mode.CONFIG_CANCEL:
    assert steps[0]['output'] == {'exception': 'CancelledError', 'message': ''}
    assert result['effects'] == [{'cancel_lock_held': True}]
  elif case.name in ('updated-dirty-owned-repo', 'prepare-pins-newly-fetched-target', 'cooldown-just-before-and-at-300'):
    target = heads['other' if case.mode == Mode.TARGET_MOVE else 'target']
    assert steps[0]['output']['result'] == [True, True, target]
    assert row['repository_after']['head'] == target
    if len(steps) == 3:
      assert steps[1]['output']['result'] == [False, False, '']
      assert steps[2]['output']['result'] == [True, True, target]
  else:
    assert all(step['output']['result'] == [False, False, ''] for step in steps)
  if case.busy or case.index_lock or case.mode == Mode.RESET_BUSY:
    for step in steps:
      state = step['state'].get('auto_update', {})
      assert state.get('status') == 'waiting' if case.pulling or case.mode == Mode.RESET_BUSY else 'auto_update' not in step['state']
      if state.get('status') == 'waiting':
        assert state['error_code'] == 'git_busy'


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--case', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  environment = os.environ | {'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null'}
  os.environ.update(environment)
  base, heads = template(args.output, environment)
  comparisons = []
  for case in CASES:
    if args.case and case.name not in args.case:
      continue
    source = execute(case, args.output / case.name / 'source', base, heads, None)
    expected(case, source, heads)
    comparison = {'name': case.name, 'source_passed': True}
    if args.binary:
      native = execute(case, args.output / case.name / 'native', base, heads, args.binary.resolve())
      expected(case, native, heads)
      left, right = observable(source), observable(native)
      assert left == right, (case.name, left, right)
      comparison['equal'] = True
    comparisons.append(comparison)
    print(json.dumps(comparison), flush=True)
    (args.output / 'result.json').write_text(json.dumps({'cases': comparisons, 'heads': heads}, indent=2) + '\n')


if __name__ == '__main__':
  main()
