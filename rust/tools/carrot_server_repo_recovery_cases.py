from __future__ import annotations

from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import subprocess
import sys
from typing import Literal, TypedDict, assert_never

from carrot_server_repo_recovery_source import Action, Config

ROOT = Path(__file__).resolve().parents[2]
BASE = 1700000000


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  index: Literal['regular', 'missing', 'directory', 'symlink'] = 'regular'
  age: float = 61
  process: str = 'other'
  action: Action = 'none'
  git: str = 'normal'
  cancel: bool = False


CASES = (
  Case('missing', index='missing'), Case('fresh', age=59.999), Case('boundary', age=60),
  Case('stale'), Case('directory', index='directory'), Case('symlink', index='symlink'),
  Case('git', process='git'), Case('git_dash', process='git-fetch'), Case('other', process='github'),
  Case('proc_missing', process='missing'), Case('proc_denied', process='denied'),
  Case('comm_denied', process='comm_denied'), Case('comm_vanished', process='vanished'),
  Case('recheck_removed', action='remove'), Case('recheck_inode', action='replace'),
  Case('recheck_size', action='size'), Case('recheck_mtime_ns', action='mtime'),
  Case('recheck_git', action='git_start'), Case('recheck_proc_missing', action='proc_remove'),
  Case('lookup_stderr', git='stderr'), Case('lookup_empty_error', git='empty_error'),
  Case('stdout_strict', git='stdout_utf8'), Case('stderr_strict', git='stderr_utf8'),
  Case('universal_newline', git='newline'), Case('deadline_10', git='timeout'),
  Case('real_git', process='real'), Case('cancellation_shield', cancel=True),
)


def setup(directory: Path, case: Case) -> Config:
  repo = directory / 'repo'
  repo.mkdir(parents=True)
  subprocess.run(['/usr/bin/git', 'init', '-q', str(repo)], check=True)
  top = subprocess.check_output(['/usr/bin/git', '-C', str(repo), 'rev-parse', '--show-toplevel'], text=True)
  assert Path(top.strip()).resolve() == repo.resolve() and (repo / '.git').is_dir()
  lock = directory / 'repo.lock'
  lock.write_text('preserve cooperative lock\n')
  for name in ('config.lock', 'HEAD.lock', 'refs/heads/owned.lock'):
    path = repo / '.git' / name
    path.parent.mkdir(exist_ok=True)
    path.write_text('preserve unrelated lock\n')
  index = repo / '.git/index.lock'
  match case.index:
    case 'regular':
      index.write_bytes(b'owned index lock\n')
      os.utime(index, ns=(BASE * 10**9, BASE * 10**9))
    case 'directory':
      index.mkdir()
    case 'symlink':
      index.symlink_to(repo / '.git/config')
    case 'missing':
      pass
    case unreachable:
      assert_never(unreachable)
  proc = directory / 'proc'
  proc.mkdir()
  if case.process not in ('missing', 'real'):
    process = proc / '42'
    process.mkdir()
    if case.process != 'vanished':
      (process / 'comm').write_text(case.process + '\n')
    if case.process == 'comm_denied':
      (process / 'comm').chmod(0)
    if case.process == 'denied':
      proc.chmod(0)
  if case.process == 'missing':
    proc.rmdir()
  if case.process == 'real':
    proc = Path('/proc')
  bindir = directory / 'bin'
  bindir.mkdir()
  wrapper = bindir / 'git'
  wrapper.write_text(f'#!{sys.executable}\n' + (ROOT / 'rust/tools/carrot_server_repo_recovery_git.py').read_text())
  wrapper.chmod(0o700)
  return {'repo': str(repo), 'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'),
          'lock': str(lock), 'proc_root': str(proc), 'now': BASE + case.age,
          'action': case.action, 'cancel': case.cancel}


def protected(directory: Path) -> dict[str, str]:
  names = ('repo.lock', 'repo/.git/config', 'repo/.git/config.lock', 'repo/.git/HEAD.lock', 'repo/.git/refs/heads/owned.lock')
  return {name: hashlib.sha256((directory / name).read_bytes()).hexdigest() for name in names}


class IndexState(TypedDict, total=False):
  kind: Literal['symlink', 'directory', 'file', 'missing']
  data: str
  mtime_ns: int


def index_state(directory: Path) -> IndexState:
  path = directory / 'repo/.git/index.lock'
  if path.is_symlink():
    return {'kind': 'symlink'}
  if path.is_dir():
    return {'kind': 'directory'}
  if path.is_file():
    return {'kind': 'file', 'data': path.read_bytes().hex(), 'mtime_ns': path.stat().st_mtime_ns}
  return {'kind': 'missing'}
