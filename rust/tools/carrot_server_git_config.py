#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# uv run rust/tools/carrot_server_git_config.py BINARY OUTPUT [CASE ...]
# The process-child helper must be beside BINARY or set OP_PROCESS_CHILD.
# ──────────────────
"""Compare the source operations with the native owned-process fixture."""
from __future__ import annotations

from contextlib import nullcontext
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import TypedDict

from carrot_server_git_config_cases import BRANCH, CASES, WRAPPER, Case

ROOT = Path(__file__).resolve().parents[2]


class Snapshot(TypedDict):
  config: str | None
  head: str
  branch: str
  index: str
  status: str
  version: str
  untracked: str | None
  merges: str
  upstream: str
  fetch_head: str | None


class Receipt(TypedDict):
  input: dict[str, str | bool | None]
  output: dict[str, list[str | int] | str]
  before: Snapshot
  after: Snapshot
  argv: list[list[str]]
  lock_records: list[dict[str, str | bool | int | list[str]]]
  invocation: list[str]
  exit_code: int
  stderr: str


def git(repo: Path, *args: str, check: bool = True) -> str:
  result = subprocess.run(['/usr/bin/git', '-C', str(repo), *args], capture_output=True, check=check)
  return result.stdout.decode('utf-8', errors='replace').strip()


def snapshot(repo: Path) -> Snapshot:
  config, version, untracked, fetched = (repo / name for name in ('.git/config', 'version.txt', 'untracked.txt', '.git/FETCH_HEAD'))
  return Snapshot(config=config.read_bytes().hex() if config.is_file() else None,
    head=git(repo, 'rev-parse', 'HEAD', check=False), branch=git(repo, 'branch', '--show-current', check=False),
    index=git(repo, 'write-tree', check=False), status=git(repo, 'status', '--porcelain', check=False),
    version=version.read_text() if version.is_file() else '', untracked=untracked.read_text() if untracked.is_file() else None,
    merges=git(repo, 'config', '--get-all', f'branch.{BRANCH}.merge', check=False),
    upstream=git(repo, 'rev-parse', '--abbrev-ref', '@{upstream}', check=False),
    fetch_head=fetched.read_text() if fetched.is_file() else None)


def source() -> None:
  from openpilot.common.repo_update import repo_lock
  from openpilot.selfdrive.carrot.server.services.git_config import prepare_git_pull, repair_git_config
  config = json.load(sys.stdin)
  with repo_lock() if config['lock'] else nullcontext():
    match config['operation']:
      case 'repair':
        result = repair_git_config(config['repo'], remote=config['remote'], repair_upstream=config['repair_upstream'])
      case 'prepare':
        result = prepare_git_pull(config['repo'])
      case unknown:
        raise AssertionError(f'unknown fixture operation {unknown}')
  print(json.dumps({'result': result}))


def setup(root: Path, case: Case) -> Path:
  root.mkdir(parents=True)
  seed, remote = root / 'seed', root / 'origin.git'
  seed.mkdir()
  git(seed, 'init', '-b', BRANCH)
  git(seed, 'config', 'user.name', 'Owned Git fixture')
  git(seed, 'config', 'user.email', 'fixture@example.invalid')
  (seed / 'version.txt').write_text('old\n')
  git(seed, 'add', 'version.txt')
  git(seed, 'commit', '-qm', 'initial')
  git(root, 'clone', '--bare', str(seed), str(remote))
  for side in ('source', 'native'):
    repo = root / side / 'repo'
    repo.parent.mkdir()
    git(root, 'clone', '--depth=1', '--branch', BRANCH, remote.as_uri(), str(repo))
    for command in case.commands:
      git(repo, *command)
    if case.dirty:
      (repo / 'version.txt').write_text('staged local change\n')
      git(repo, 'add', 'version.txt')
      (repo / 'untracked.txt').write_text('keep local file\n')
  (seed / 'version.txt').write_text('new\n')
  git(seed, 'commit', '-qam', 'update')
  git(seed, 'branch', 'other-model', 'HEAD~1')
  git(seed, 'push', str(remote), BRANCH, 'other-model')
  return remote


def execute(root: Path, case: Case, side: str, binary: Path, env: dict[str, str]) -> Receipt:
  directory = root / side
  repo = directory / 'repo'
  assert (repo / '.git').is_dir(), repo
  assert git(repo, 'rev-parse', '--show-toplevel') == str(repo.resolve()), repo
  if case.mode == 'unreachable':
    git(repo, 'remote', 'set-url', 'origin', str(root / 'missing.git'))
  if case.mode == 'missing-cwd':
    repo = directory / 'missing'
  if case.mode == 'file-cwd':
    repo = directory / 'file'
    repo.write_text('owned non-directory')
  before = snapshot(repo)
  bindir = directory / 'bin'
  bindir.mkdir()
  if case.mode != 'missing-git':
    wrapper = bindir / 'git'
    wrapper.write_text(WRAPPER)
    wrapper.chmod(0 if case.mode == 'permission-git' else 0o700)
  trace = directory / 'argv.jsonl'
  lock = directory / 'repo.lock' if case.lock else None
  command_env = {**env, 'PATH': str(bindir), 'OP_GIT_FIXTURE_MODE': case.mode,
                 'OP_GIT_FIXTURE_TRACE': str(trace), 'OP_GIT_FIXTURE_GROUP': str(os.getpgrp())}
  if lock:
    command_env.update(CARROT_REPO_LOCK_PATH=str(lock), OP_GIT_FIXTURE_LOCK=str(lock))
  config = {'operation': case.operation, 'repo': str(repo), 'launcher': env['OP_PROCESS_CHILD'],
            'remote': case.remote, 'repair_upstream': case.upstream, 'lock': str(lock) if lock else None}
  command = [sys.executable, '-P', str(Path(__file__).resolve()), '--source'] if side == 'source' else [str(binary)]
  if case.mode == 'permission-cwd':
    repo.chmod(0)
  try:
    process = subprocess.run(command, input=json.dumps(config), text=True, capture_output=True, env=command_env, timeout=30, check=True)
  finally:
    if case.mode == 'permission-cwd':
      repo.chmod(0o755)
  rows = [json.loads(line) for line in trace.read_text().splitlines()] if trace.exists() else []
  receipt = Receipt(input=config, output=json.loads(process.stdout), before=before, after=snapshot(repo),
                    argv=[row['args'] for row in rows], lock_records=rows, invocation=command,
                    exit_code=process.returncode, stderr=process.stderr)
  (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  return receipt


def main() -> None:
  if sys.argv[1:2] == ['--source']:
    source()
    return
  binary, output = (Path(value).resolve() for value in sys.argv[1:3])
  selected = set(sys.argv[3:])
  output.mkdir(parents=True, exist_ok=False)
  env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_TERMINAL_PROMPT': '0',
         'GIT_AUTHOR_DATE': '2000-01-01T00:00:00+00:00', 'GIT_COMMITTER_DATE': '2000-01-01T00:00:00+00:00',
         'PYTHONPATH': f'{ROOT / "rust/tools"}:{ROOT}',
         'OP_PROCESS_CHILD': os.environ.get('OP_PROCESS_CHILD', str(ROOT / 'rust/target/debug/openpilot-process-child'))}
  os.environ.update(env)
  results = []
  for case in CASES:
    if selected and case.name not in selected:
      continue
    root = output / case.name
    remote = setup(root, case)
    global_config = root / 'global.config'
    global_config.write_text(case.global_config)
    case_env = {**env, 'GIT_CONFIG_GLOBAL': str(global_config)}
    receipts = [execute(root, case, side, binary, case_env) for side in ('source', 'native')]
    normalized = [json.dumps(receipt, sort_keys=True).replace(str(root / side), '<SIDE>')
                  for receipt, side in zip(receipts, ('source', 'native'), strict=True)]
    compared = [json.loads(raw) for raw in normalized]
    equal = all(compared[0][key] == compared[1][key] for key in ('output', 'before', 'after', 'argv'))
    preserved = all(row['before'][key] == row['after'][key] for row in receipts for key in ('head', 'branch', 'index', 'status', 'version', 'untracked'))
    locks = not case.lock or all(row.get('lock_fds') == 1 and row.get('held') is True and row.get('same_group') is True
                                for receipt in receipts for row in receipt['lock_records'])
    pin = True
    if case.name == 'pinned-target':
      expected = git(remote, 'rev-parse', f'refs/heads/{BRANCH}')
      for receipt, side in zip(receipts, ('source', 'native'), strict=True):
        repo = root / side / 'repo'
        git(repo, 'fetch', 'origin', 'other-model')
        target, fetched = receipt['output']['result'][2], git(repo, 'rev-parse', 'FETCH_HEAD')
        pinned = target == expected and fetched != target
        pin = pin and pinned
        (root / side / 'pin.json').write_text(json.dumps({'target': target, 'expected': expected,
          'unrelated_fetch_head': fetched, 'pinned': pinned}, indent=2) + '\n')
    results.append({'case': case.name, 'equal': equal, 'checkout_preserved': preserved, 'lock_forwarded': locks,
                    'pinned_commit': pin, 'passed': equal and preserved and locks and pin})
    (output / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
    print(case.name, 'PASS' if results[-1]['passed'] else 'FAIL', flush=True)
  identity = {'binary': {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()},
              'source_sha256': hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/server/services/git_config.py').read_bytes()).hexdigest(),
              'process_child_sha256': hashlib.sha256(Path(env['OP_PROCESS_CHILD']).read_bytes()).hexdigest()}
  (output / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
  raise SystemExit(0 if results and all(row['passed'] for row in results) else 1)


if __name__ == '__main__':
  main()
