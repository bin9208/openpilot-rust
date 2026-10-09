#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# Imported by: uv run rust/tools/carrot_server_git_config.py BINARY OUTPUT
# ──────────────────
"""Owned Git fixtures for the original repair and pinned-pull operations."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass(frozen=True, slots=True)
class Case:
  name: str
  commands: tuple[tuple[str, ...], ...] = ()
  operation: Literal['repair', 'prepare'] = 'repair'
  remote: str | None = None
  upstream: bool = True
  global_config: str = ''
  mode: str = ''
  dirty: bool = False
  lock: bool = False


BRANCH = 'carrot-wip'
OLD = '+refs/heads/deleted:refs/remotes/origin/deleted'
CASES = (
  Case('verified'),
  Case('migrated', (('config', '--replace-all', 'remote.origin.fetch', OLD),
                    ('config', f'branch.{BRANCH}.merge', 'refs/heads/deleted'))),
  Case('duplicate-obsolete-dirty', tuple(('config', '--add', 'remote.origin.fetch', spec)
       for spec in (OLD, OLD, '+refs/tags/*:refs/tags/*', '^refs/heads/private/*')), dirty=True),
  Case('renamed', (('remote', 'rename', 'origin', 'my-fork'), ('branch', '-m', 'my-local')), operation='prepare'),
  Case('missing-upstream', (('branch', '--unset-upstream'),)),
  Case('missing-model', (('branch', '-m', 'unknown-model'),
                         ('config', 'branch.unknown-model.merge', 'refs/heads/deleted')), operation='prepare'),
  Case('upstream-disabled', (('branch', '-m', 'release-staging'),
       ('config', '--replace-all', 'remote.origin.fetch', OLD)), remote='origin', upstream=False),
  Case('detached', (('checkout', '--detach'),)),
  Case('no-remote', (('remote', 'remove', 'origin'),)),
  Case('no-remote-prepare', (('remote', 'remove', 'origin'),), operation='prepare'),
  Case('local-upstream', (('config', f'branch.{BRANCH}.remote', '.'),), operation='prepare'),
  Case('missing-selected', remote='missing'),
  Case('empty-selection', remote=''),
  Case('no-origin-unconfigured', (('remote', 'rename', 'origin', 'fork'), ('branch', '--unset-upstream'))),
  Case('explicit-selection', (('remote', 'rename', 'origin', 'fork'), ('branch', '--unset-upstream')), remote='fork'),
  Case('duplicate-same-renamed', (('branch', '-m', 'my-local'), ('remote', 'rename', 'origin', 'my-fork'),
       ('config', '--add', 'branch.my-local.merge', f'refs/heads/{BRANCH}')), operation='prepare'),
  Case('duplicate-different', (('config', '--add', f'branch.{BRANCH}.merge', 'refs/heads/other-model'),), operation='prepare'),
  Case('ambiguous-model', (('branch', '-m', 'unknown-model'),
       ('config', '--add', 'branch.unknown-model.merge', 'refs/heads/other-model')), operation='prepare'),
  Case('inherited-merge', global_config=f'[branch "{BRANCH}"]\nmerge = refs/heads/other-model\n'),
  Case('inherited-obsolete', global_config=f'[remote "origin"]\nfetch = {OLD}\n'),
  Case('excluded-target', (('config', '--add', 'remote.origin.fetch', f'^refs/heads/{BRANCH}'),)),
  Case('pinned-target', operation='prepare'),
  Case('lock-forwarding', operation='prepare', lock=True),
  Case('fetch-failure', (('config', '--add', f'branch.{BRANCH}.merge', 'refs/heads/other-model'),), mode='fetch-failure', operation='prepare'),
  Case('unreachable-remote', mode='unreachable', dirty=True),
  Case('advertised-race', mode='advertised-race'),
  Case('output-replacement', mode='output-error'),
  Case('empty-command-error', mode='empty-error'),
  Case('signal-command-error', mode='signal-error'),
  Case('pin-stderr-only', mode='pin-output-error', operation='prepare'),
  Case('pin-negative-signal', mode='pin-signal-error', operation='prepare'),
  Case('missing-cwd', mode='missing-cwd'),
  Case('file-cwd', mode='file-cwd'),
  Case('permission-cwd', mode='permission-cwd'),
  Case('permission-git', mode='permission-git'),
  Case('missing-git', mode='missing-git'),
)


WRAPPER = r'''#!/usr/bin/python3
import errno, fcntl, json, os, signal, subprocess, sys
from pathlib import Path
args = sys.argv[1:]
mode = os.environ.get('OP_GIT_FIXTURE_MODE', '')
lock = os.environ.get('OP_GIT_FIXTURE_LOCK', '')
record = {'args': args}
if lock:
  target = os.stat(lock)
  matches = []
  for item in Path('/proc/self/fd').iterdir():
    try:
      info = os.stat(item)
      if (info.st_dev, info.st_ino) == (target.st_dev, target.st_ino): matches.append(int(item.name))
    except FileNotFoundError: continue
  with open(lock, 'rb') as contender:
    try:
      fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
      held = False
    except BlockingIOError: held = True
  record.update(lock_fds=len(matches), held=held, same_group=os.getpgrp()==int(os.environ['OP_GIT_FIXTURE_GROUP']))
with open(os.environ['OP_GIT_FIXTURE_TRACE'], 'a') as trace:
  trace.write(json.dumps(record)+'\n')
if args[0] == 'symbolic-ref':
  if mode == 'output-error':
    os.write(1, b' \r\nout\xff\r'); os.write(2, b' \x1cerr\xfe \x1f\r\n'); sys.exit(7)
  if mode == 'empty-error': sys.exit(9)
  if mode == 'signal-error': os.kill(os.getpid(), signal.SIGTERM)
if args[0] == 'fetch' and mode == 'fetch-failure':
  os.write(1, b' fetch stdout\r\n'); os.write(2, b'fetch stderr\xff\r'); sys.exit(4)
if args == ['ls-remote', '--heads', 'origin'] and mode == 'advertised-race':
  result = subprocess.run(['/usr/bin/git', *args], capture_output=True)
  os.write(1, result.stdout.replace(result.stdout.split()[0], b'0'*40, 1)); sys.exit(result.returncode)
if args == ['rev-parse', '--verify', '@{upstream}^{commit}']:
  if mode == 'pin-output-error':
    os.write(1, b'ignored stdout\r\n'); os.write(2, b' pin stderr\xfe \r'); sys.exit(6)
  if mode == 'pin-signal-error': os.kill(os.getpid(), signal.SIGTERM)
os.execv('/usr/bin/git', ['git', *args])
'''
