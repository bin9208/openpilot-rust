from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import sys
import time

from carrot_server_git_status_cases import Fixtures
from carrot_server_git_status_peer import Peer


def assert_exited(peer: Peer) -> None:
  pids = [entry['pid'] for entry in peer.git_trace()]
  if peer.descendant.exists():
    pids.append(int(peer.descendant.read_text()))
  for pid in pids:
    try:
      fd = os.pidfd_open(pid)
    except ProcessLookupError:
      continue
    try:
      poll = select.poll()
      poll.register(fd, select.POLLIN)
      if not poll.poll(1000):
        raise RuntimeError(f'owned Git child remains alive: {pid}')
    finally:
      os.close(fd)


def cache_scenario(peer: Peer, fixtures: Fixtures, repo: Path) -> list:
  responses = [peer.request('get', force=False, now=1000.25)]
  original = fixtures.git(repo, 'rev-parse', 'HEAD')
  fixtures.commit(repo, 'cache-added')
  responses.append(peer.request('get', force=False, now=1599.999))
  responses.append(peer.request('get', force=False, now=1600.0))
  responses.append(peer.request('get', force=False, now=900.0))
  responses.append(peer.request('get', force=True, now=900.0))
  responses.append(peer.request('clear'))
  responses.append(peer.request('get', force=False, now=900.0))
  if responses[0] != responses[1] or responses[2]['head'] == original or responses[2] != responses[3]:
    raise RuntimeError('source TTL boundary/backward-clock control failed')
  fixtures.git(repo, 'reset', '--hard', original)
  (repo / 'cache-added').unlink(missing_ok=True)
  return responses


def busy_scenario(peer: Peer, fixtures: Fixtures, repo: Path) -> list:
  responses = [peer.request('get', force=False, now=1000.0)]
  with open(peer.config['lock'], 'a+') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    responses.append(peer.request('get', force=False, now=1001.0))
    responses.append(peer.request('get', force=True, now=1001.0))
  responses.append(peer.request('get', force=False, now=1002.0))
  responses.append(peer.request('get', force=True, now=1002.0))
  if responses[0] != responses[1] or responses[0] != responses[3] or responses[2]['state'] != 'busy':
    raise RuntimeError('busy must preserve cached state')
  return responses


def poll_scenario(peer: Peer) -> list:
  responses = [peer.request('poll', interval_ms=60, initial_ms=30)]
  deadline = time.monotonic() + 5
  while sum(row['args'][0] == 'fetch' for row in peer.git_trace()) < 2:
    if time.monotonic() > deadline:
      raise RuntimeError('owned periodic checks did not advance')
    time.sleep(0.005)
  responses.append(peer.request('stop_poll'))
  responses.append(peer.request('get', force=False, now=1000.0))
  count = len(peer.git_trace())
  time.sleep(0.14)
  if len(peer.git_trace()) != count:
    raise RuntimeError('owned periodic child survived stop')
  return responses


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--timeouts', action='store_true')
  parser.add_argument('--case', action='append')
  args = parser.parse_args()
  free = shutil.disk_usage(args.output.parent).free
  if free < 25 * 1024 ** 3 + 128 * 1024 ** 2:
    raise RuntimeError('owned Git fixture requires 25GiB +128MiB free')
  args.output.mkdir(parents=True, exist_ok=False)
  (args.output / 'space-guard.json').write_text(json.dumps(dict(free_bytes=free, floor=25 * 1024 ** 3, growth=128 * 1024 ** 2)) + '\n')
  fixtures = Fixtures(args.output / 'fixtures')
  cases = list(fixtures.cases())
  cases += [(name, fixtures.clone(name)) for name in ['cache', 'busy', 'coalesce', 'forced', 'cancel', 'initial_cancel', 'poll', 'fetch_recovery']]
  if args.case:
    cases += [(name, fixtures.clone(name)) for name in ['queued_poll_cancel', 'poll_active_cancel'] if name in args.case]
  if args.timeouts:
    cases += [(name, fixtures.clone(name)) for name in ['git_timeout', 'fetch_timeout', 'exited_leader_pipe']]
  if args.case:
    cases = [(name, repo) for name, repo in cases if name in args.case]
  source = Path(__file__).with_name('carrot_server_git_status_source.py')
  sides = [('source', [sys.executable, '-P', str(source)])]
  if args.binary is not None:
    sides.append(('native', [str(args.binary.resolve())]))
  results: list[dict] = []
  for name, repo in cases:
    outputs = {}
    timings = {}
    for side, command in sides:
      lock_path = args.output / f'{name}-{side}.lock'
      env = dict(fixtures.env, PYTHONPATH=str(Path(__file__).resolve().parents[2]), CARROT_REPO_LOCK_PATH=str(lock_path))
      target = {'git_timeout': 'rev-parse', 'fetch_timeout': 'fetch', 'cancel': 'rev-parse', 'exited_leader_pipe': 'rev-parse', 'queued_poll_cancel': 'rev-parse', 'poll_active_cancel': 'rev-parse'}.get(name)
      if target is not None:
        env['OWNED_GIT_BLOCK'] = target
      if name == 'exited_leader_pipe':
        env['OWNED_GIT_EXIT_LEADER'] = '1'
      root = args.output / name / side
      if name in ['cancel', 'queued_poll_cancel', 'poll_active_cancel']:
        env['OWNED_GIT_NOTICE'] = str(root / 'notice')
      peer = Peer(command, repo, root, env, args.launcher.resolve())
      try:
        match name:
          case 'cache': output = cache_scenario(peer, fixtures, repo)
          case 'busy': output = busy_scenario(peer, fixtures, repo)
          case 'coalesce' | 'forced':
            output = peer.request('group', force=name == 'forced', now=1000.0, count=3)
            fetches = sum(row['args'][0] == 'fetch' for row in peer.git_trace())
            if fetches != (3 if name == 'forced' else 1):
              raise RuntimeError(f'owned serialized refresh count differs: {fetches}')
          case 'cancel':
            os.mkfifo(root / 'notice')
            output = peer.request('cancel', notice=str(root / 'notice'))
            assert_exited(peer)
          case 'initial_cancel':
            output = peer.request('cancel_immediate')
            assert_exited(peer)
          case 'queued_poll_cancel':
            os.mkfifo(root / 'notice')
            output = peer.request('queued_poll_cancel', notice=str(root / 'notice'))
            assert_exited(peer)
          case 'poll_active_cancel':
            os.mkfifo(root / 'notice')
            peer.request('poll', interval_ms=60, initial_ms=0)
            notice = os.open(root / 'notice', os.O_RDWR | os.O_NONBLOCK)
            try:
              if not select.select([notice], [], [], 5)[0] or os.read(notice, 1) != b'R':
                raise RuntimeError('owned periodic child did not reach notice gate')
            finally:
              os.close(notice)
            output = peer.request('stop_poll')
            assert_exited(peer)
          case 'poll': output = poll_scenario(peer)
          case 'fetch_recovery':
            fixtures.git(repo, 'remote', 'set-url', 'origin', str(fixtures.root / 'missing-owned.git'))
            output = [peer.request('get', force=True, now=1000.0)]
            fixtures.git(repo, 'remote', 'set-url', 'origin', str(fixtures.bare))
            output.append(peer.request('get', force=True, now=1001.0))
          case _:
            output = peer.request('get', force=True, now=1000.0)
            if name.endswith('_timeout') or name == 'exited_leader_pipe':
              assert_exited(peer)
              elapsed = peer.observations[-1]['elapsed']
              if elapsed < (25 if name == 'fetch_timeout' else 8) or elapsed > 30:
                raise RuntimeError(f'owned timeout outside bounded window: {elapsed}')
        if any(not row['lock_inherited'] for row in peer.git_trace()):
          raise RuntimeError('owned real Git child did not inherit repository lock')
        outputs[side] = output
        timings[side] = peer.observations[-1]['elapsed']
      finally:
        peer.close()
    if args.binary is not None and outputs['source'] != outputs['native']:
      (args.output / 'difference.json').write_text(json.dumps(dict(case=name, outputs=outputs), indent=2) + '\n')
      raise RuntimeError(f'original/native difference: {name}')
    if args.binary is not None and name in ['cancel', 'git_timeout', 'fetch_timeout', 'exited_leader_pipe'] and abs(timings['source'] - timings['native']) > 0.6:
      (args.output / 'difference.json').write_text(json.dumps(dict(case=name, elapsed=timings, boundary='one-second cleanup grace'), indent=2) + '\n')
      raise RuntimeError(f'original/native cleanup phase differs: {name}')
    results.append(dict(case=name, outputs=outputs, elapsed=timings))
    print('PASS', name, flush=True)
  fixtures.save()
  files = [source, Path(__file__), Path(__file__).with_name('carrot_server_git_status_cases.py'), Path(__file__).with_name('carrot_server_git_status_peer.py'), args.launcher]
  if args.binary is not None:
    files.append(args.binary)
  (args.output / 'result.json').write_text(json.dumps(dict(passed=True, compared=args.binary is not None, cases=results, files={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}), indent=2) + '\n')


if __name__ == '__main__':
  main()
