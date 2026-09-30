#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from checkout_git_cases import git_file, git_fixture, real_git
from checkout_helper_cases import Helpers, helper_cases, inherited_cases
from checkout_metadata_cases import metadata_cases
from checkout_peer import Binaries, Peer, enable_subreaper
from checkout_status_cases import fixture_status, packaged_status


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--captured-binary', type=Path)
  parser.add_argument('--handshake-fixture', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--group', choices=['all', 'metadata', 'git', 'status', 'helper'], default='all')
  args = parser.parse_args()
  enable_subreaper()
  args.output.mkdir(parents=True, exist_ok=True)
  binaries = Binaries(args.binary, args.fixture, args.launcher)
  groups = {
    'metadata': [('metadata', metadata_cases, False)],
    'git': [('git-sha1', real_git, False), ('git-sha256', lambda p: real_git(p, 'sha256'), False),
            ('git-file', git_file, False), ('git-executable', git_fixture, True)],
    'status': [('packaged-status', packaged_status, False), ('git-status', fixture_status, True)],
  }
  scenarios = [item for group in groups.values() for item in group] if args.group == 'all' else groups.get(args.group, [])
  results = []
  for name, run, fixture_mode in scenarios:
    pair = []
    for implementation in ['python', 'rust']:
      with Peer(binaries, implementation, args.output / name, fixture_mode) as peer:
        result = run(peer)
        pair.append({'implementation': implementation, 'result': result})
    assert pair[0]['result'] == pair[1]['result'], (name, pair)
    results.append({'scenario': name, 'passed': True, 'results': pair})
    (args.output / 'summary.json').write_text(json.dumps({'passed': False, 'results': results}, indent=2) + '\n')
    print('PASS:', name, flush=True)
  if args.group in ['all', 'helper']:
    if args.captured_binary is None or args.handshake_fixture is None:
      parser.error('all/helper checks require --captured-binary and --handshake-fixture')
    helpers = Helpers(args.captured_binary, args.handshake_fixture)
    native = helper_cases(binaries, helpers, args.output / 'native-helper')
    results.append({'scenario': 'native-helper', 'passed': True, 'results': native})
    print('PASS: native-helper', flush=True)
    inherited = inherited_cases(binaries, helpers, args.output / 'inherited-helper')
    results.append({'scenario': 'inherited-stdio', 'passed': True, 'results': inherited})
    print('PASS: inherited-stdio', flush=True)
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'scenarios': len(results), 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
  main()
