# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp"]
# ///
# Run with retained Python/PYTHONPATH from the evidence invocation; supply --binary, --launcher and --output.
from __future__ import annotations

import argparse
import fcntl
import json
from pathlib import Path
import sys

from carrot_server_git_status_cases import Fixtures
from carrot_server_tools_git_peer import Peer


def compare(args: argparse.Namespace) -> None:
  args.output.mkdir(parents=True)
  fixtures = Fixtures(args.output / 'repositories')
  source = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_tools_git_source.py'))]
  peers = []
  rows = []
  try:
    families = ('normal',) if args.composed else ('normal', 'no_upstream', 'not_git', 'fetch_error', 'busy', 'lock_error')
    for family in families:
      repository = fixtures.clone(family)
      if family == 'no_upstream':
        fixtures.git(repository, 'remote', 'remove', 'origin')
      if family == 'not_git':
        repository = fixtures.root / 'empty'
        repository.mkdir()
      if family == 'fetch_error':
        fixtures.git(repository, 'remote', 'set-url', 'origin', str(fixtures.root / 'missing-bare.git'))
      pair = [Peer(command, args.output / family / side, repository, fixtures.env, args.launcher, composed=args.composed)
              for side, command in (('source', source), ('native', [str(args.binary)]))]
      peers.extend(pair)
      locks = []
      if family == 'busy':
        for peer in pair:
          file = peer.lock.open('w')
          fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
          locks.append(file)
      if family == 'lock_error':
        for peer in pair:
          peer.lock.mkdir()
      def request(name: str, query: str = '', method: str = 'GET', headers: dict[str, str] | None = None) -> None:
        observed = []
        deltas = []
        for peer in pair:
          before = len(peer.calls())
          observed.append(peer.request('/api/tools/git_status' + query, method, headers))
          deltas.append(len(peer.calls()) - before)
        row = {'family': family, 'scenario': name, 'source': observed[0], 'native': observed[1], 'git_command_deltas': deltas}
        rows.append(row)
        (args.output / 'observations.json').write_text(json.dumps(rows, indent=2))
        if observed[0] != observed[1] or deltas[0] != deltas[1]:
          raise AssertionError(f'actual HTTP or command mismatch: {family}/{name}')
      request('initial')
      request('head', method='HEAD')
      request('post', method='POST')
      if family == 'normal':
        queries = ('', '?force=1', '?force=true', '?force=YES', '?refresh=TRUE', '?force=0&refresh=yes',
                   '?force=0', '?force=on', '?force=no&force=yes', '?refresh=&refresh=yes',
                   '?force=%20YeS%20', '?force=%1Ctrue%1F', '?force=&refresh=%C2%A0yes%C2%A0',
                   '?FORCE=yes', '?force=yes&force=no', '?refresh=yes&refresh=no')
        for index, query in enumerate(queries if not args.composed else ('?force=no&force=yes', '?refresh=yes')):
          request(f'query-{index}', query)
        request('no_compression', headers={'Accept-Encoding': 'deflate,gzip,br'})
        for peer in pair:
          (peer.root / 'data/state/git.json').write_text('{"auto_update":{"status":"changed","other":8}}')
        request('state_after_cached_status')
        for peer in pair:
          (peer.root / 'data/state/git.json').write_text('{invalid')
        request('invalid_state')
        for peer in pair:
          (peer.root / 'data/state/git.json').write_text('{"auto_update":["wrong-type"]}')
        request('non_object_state')
      if family in ('busy', 'lock_error', 'fetch_error'):
        for file in locks:
          file.close()
        if family == 'lock_error':
          for peer in pair:
            peer.lock.rmdir()
        if family == 'fetch_error':
          fixtures.git(repository, 'remote', 'set-url', 'origin', str(fixtures.bare))
        request('recovery', '?force=yes')
      for peer in pair:
        peer.stop()
        peers.remove(peer)
    result = {'pairs': len(rows), 'failures': 0, 'scope': 'original Tools Git route + actual owned Git/lock/state files',
              'composed': args.composed, 'all_peers_exit_zero': True}
    (args.output / 'result.json').write_text(json.dumps(result, indent=2))
    (args.output / 'fixture-git-commands.json').write_text(json.dumps(fixtures.commands, indent=2))
    print(json.dumps(result))
  finally:
    for peer in peers:
      if peer.process.poll() is None:
        peer.stop()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--composed', action='store_true')
  parser.add_argument('--connection-only', action='store_true')
  args = parser.parse_args()
  if args.connection_only:
    connection(args)
  else:
    compare(args)


def connection(args: argparse.Namespace) -> None:
  args.output.mkdir(parents=True)
  fixtures = Fixtures(args.output / 'repositories')
  repository = fixtures.clone('error')
  source = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_tools_git_source.py'))]
  rows = []
  for side, command in (('source', source), ('native', [str(args.binary)])):
    peer = Peer(command, args.output / side, repository, fixtures.env, args.launcher, composed=True)
    peer.lock.mkdir()
    try:
      rows.append(peer.error_connection())
    finally:
      peer.stop()
  result = {'passed': rows[0] == rows[1] and rows[1]['status'] == 500 and rows[1]['eof'],
            'source': rows[0], 'native': rows[1]}
  (args.output / 'result.json').write_text(json.dumps(result, indent=2))
  print(json.dumps(result))
  if not result['passed']:
    raise AssertionError('generic Git error did not close the owned keepalive connection')


if __name__ == '__main__':
  main()
