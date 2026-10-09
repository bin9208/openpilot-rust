from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
import json
from pathlib import Path
import shutil
import subprocess
from typing import TypedDict


class Mode(StrEnum):
  NORMAL = 'normal'
  RESET_BUSY = 'reset-busy'
  MERGE_BUSY = 'merge-busy'
  RESET_OUTPUT = 'reset-output'
  RESET_SIGNAL = 'reset-signal'
  POST_HEAD_FAIL = 'post-head-fail'
  POST_HEAD_MISMATCH = 'post-head-mismatch'
  UPDATED_STATE_FAIL = 'updated-state-fail'
  CANCEL_GROUP = 'cancel-group'
  CANCEL_NO_READY = 'cancel-no-ready'
  RESET_EXEC_DENIED = 'reset-exec-denied'


class Snapshot(TypedDict):
  head: str
  tracked: str | None
  fetch_head: str | None


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  mode: Mode = Mode.NORMAL
  base: str = 'committed'
  target: str = 'target'
  receipt: str = ''
  blocked_state: bool = False
  alert_failure: bool = False
  notify_failure: bool = False


CASES = (
  Case('no-target', target='empty'),
  Case('duplicate-reboot', receipt='requested'),
  Case('duplicate-reboot-already-blocked', receipt='blocked'),
  Case('unborn-head', base='unborn'),
  Case('attempt-state-save-fails', blocked_state=True),
  Case('pinned-target-dirty-reset'),
  Case('unchanged-head', target='base'),
  Case('divergent-fast-forward-fails', base='divergent'),
  Case('reset-active-index-lock', mode=Mode.RESET_BUSY),
  Case('merge-active-index-lock', mode=Mode.MERGE_BUSY),
  Case('reset-separate-writes-utf8-replacement', mode=Mode.RESET_OUTPUT),
  Case('reset-negative-signal-code', mode=Mode.RESET_SIGNAL),
  Case('post-pull-head-read-fails', mode=Mode.POST_HEAD_FAIL),
  Case('post-pull-head-mismatch', mode=Mode.POST_HEAD_MISMATCH),
  Case('verified-state-save-fails', mode=Mode.UPDATED_STATE_FAIL),
  Case('alert-failure-keeps-verified-update', alert_failure=True),
  Case('notification-failure-keeps-verified-update', notify_failure=True),
  Case('cancel-held-group', mode=Mode.CANCEL_GROUP),
  Case('reset-executable-permission-fails', mode=Mode.RESET_EXEC_DENIED),
  Case('merge-target-nul', target='nul'),
)


def assert_repository(repository: Path) -> None:
  assert (repository / '.git').is_dir(), 'Fixture must have an independent .git directory'
  top = subprocess.check_output(['/usr/bin/git', 'rev-parse', '--show-toplevel'], cwd=repository, text=True).strip()
  assert Path(top).resolve() == repository.resolve(), 'Fixture escaped its owned repository'


def git(repository: Path, *args: str) -> str:
  assert_repository(repository)
  return subprocess.check_output(['/usr/bin/git', *args], cwd=repository, text=True).strip()


def template(output: Path, environment: dict[str, str]) -> tuple[Path, dict[str, str]]:
  repository = output / 'template'
  repository.mkdir()
  subprocess.run(['/usr/bin/git', 'init', '-q', '-b', 'owned-update'], cwd=repository, env=environment, check=True)
  for key, value in [('user.name', 'Owned updater fixture'), ('user.email', 'owned@example.invalid')]:
    git(repository, 'config', key, value)
  heads = {}
  for name in ('base', 'target', 'other'):
    (repository / 'tracked.txt').write_text(name + '\n')
    assert_repository(repository)
    subprocess.run(['/usr/bin/git', 'add', 'tracked.txt'], cwd=repository, env=environment, check=True)
    subprocess.run(['/usr/bin/git', 'commit', '-qm', name], cwd=repository, env=environment, check=True)
    heads[name] = git(repository, 'rev-parse', 'HEAD')
  return repository, heads


def prepare(case: Case, directory: Path, base: Path, heads: dict[str, str], environment: dict[str, str]) -> tuple[Path, Path, str]:
  repository = directory / 'repository'
  state = directory / 'state'
  if case.base == 'unborn':
    repository.mkdir()
    subprocess.run(['/usr/bin/git', 'init', '-q', '-b', 'owned-update'], cwd=repository, env=environment, check=True)
  else:
    shutil.copytree(base, repository)
    git(repository, 'reset', '--hard', heads['base'])
    if case.base == 'divergent':
      (repository / 'diverged.txt').write_text('owned divergent commit\n')
      git(repository, 'add', 'diverged.txt')
      subprocess.run(['/usr/bin/git', 'commit', '-qm', 'diverged'], cwd=repository, env=environment, check=True)
    (repository / 'tracked.txt').write_text('owned dirty edit\n')
    (repository / '.git/FETCH_HEAD').write_text(heads['other'] + '\t\tbranch owned-other\n')
  assert_repository(repository)
  if case.mode == 'reset-busy':
    (repository / '.git/index.lock').write_text('owned active index lock\n')
  target = '' if case.target == 'empty' else 'bad\0target' if case.target == 'nul' else heads[case.target]
  if case.blocked_state:
    state.write_text('owned path that prevents state directory creation\n')
  else:
    state.mkdir()
    initial = {'unrelated': 'preserved'}
    if case.receipt:
      initial['auto_update'] = {
        'status': 'reboot_blocked' if case.receipt == 'blocked' else 'reboot_requested',
        'reboot_requested_head': target,
        'target_head': target if case.receipt == 'blocked' else 'previous-target',
      }
    (state / 'git.json').write_text(json.dumps(initial))
  return repository, state, target


def snapshot(repository: Path) -> Snapshot:
  assert_repository(repository)
  run = subprocess.run(['/usr/bin/git', 'rev-parse', 'HEAD'], cwd=repository, text=True, capture_output=True, check=False)
  return {
    'head': run.stdout.strip() if run.returncode == 0 else '',
    'tracked': (repository / 'tracked.txt').read_text() if (repository / 'tracked.txt').exists() else None,
    'fetch_head': (repository / '.git/FETCH_HEAD').read_text() if (repository / '.git/FETCH_HEAD').exists() else None,
  }
