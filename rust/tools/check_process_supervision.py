#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from supervision_cases import exec_failure, explicit_signal, kill_timeout, launch_context, nonblocking_once, restart, start_while_stopping
from supervision_peer import Peer, enable_subreaper
from supervision_ensure_cases import dead_first, ordered_ensure, predicate_race
from supervision_persistent_cases import (formatted_pid, identity_mismatch, inherited_descriptor, invalid_cmdline_utf8, invalid_pid, persistent_spawn_error,
                                          path_search, pid_overflow, reusable_daemon, unreadable_pid_and_failed_write)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--fixture', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--group', choices=['all', 'lifecycle', 'ensure', 'persistent'], default='all')
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  enable_subreaper()
  lifecycle = [('launch-context', launch_context), ('nonblocking-once', nonblocking_once),
               ('explicit-signal', explicit_signal), ('sigkill-policy', lambda p: explicit_signal(p, True)),
               ('retry-kill', lambda p: kill_timeout(p, True)), ('no-retry', lambda p: kill_timeout(p, False)),
               ('start-while-stopping', start_while_stopping), ('restart', restart), ('native-path-search', path_search)]
  lifecycle.append(('native-descriptor-inheritance', inherited_descriptor))
  invalid_launches = ['target', 'cwd', 'empty', 'nul', 'format', 'permission', 'directory', 'name-cwd', 'nul-cwd']
  lifecycle.extend(('invalid-' + kind, lambda peer, kind=kind: exec_failure(peer, kind)) for kind in invalid_launches)
  ensure = [('ensure-ordered', ordered_ensure), ('dead-no-restart-flag', lambda p: dead_first(p, False)),
            ('dead-restart-flag', lambda p: dead_first(p, True)), ('race-no-restart', lambda p: predicate_race(p, False)),
            ('race-restart', lambda p: predicate_race(p, True))]
  persistent = [('persistent-reuse', reusable_daemon), ('persistent-wrong-identity', lambda p: identity_mismatch(p, False)),
                ('persistent-dead-pid', lambda p: identity_mismatch(p, True)), ('persistent-formatted-pid', formatted_pid),
                ('persistent-spawn-error', persistent_spawn_error), ('persistent-read-write-error', unreadable_pid_and_failed_write),
                ('persistent-format-error', lambda p: persistent_spawn_error(p, True)),
                ('persistent-path-search', lambda p: path_search(p, True)),
                ('persistent-closes-descriptors', lambda p: inherited_descriptor(p, True)),
                ('persistent-cmdline-utf8', invalid_cmdline_utf8),
                ('persistent-pid-overflow', lambda p: pid_overflow(p, b'2147483648')),
                ('persistent-pid-wide', lambda p: pid_overflow(p, b'9' * 4300))]
  invalid = [('missing', None, False), ('empty', b'', False), ('text', b'x', True), ('nul', b'6\0tail', True),
             ('utf8', b'\xd9\xa6', True), ('suffix', b'6tail', True), ('digit-limit', b'9' * 4301, True)]
  persistent.extend(('persistent-' + name, lambda p, value=value, warning=warning: invalid_pid(p, value, warning)) for name, value, warning in invalid)
  groups = {'lifecycle': lifecycle, 'ensure': ensure, 'persistent': persistent}
  scenarios = lifecycle + ensure + persistent if args.group == 'all' else groups[args.group]
  results = []
  for name, run in scenarios:
    pair = []
    for implementation in ['python', 'rust']:
      with Peer(args, implementation, name) as peer:
        result = run(peer)
        peer.close_supervisor()
        messages = [(row['level'], row['msg']) for row in peer.records]
        if implementation == 'rust':
          assert all(row['ctx']['runtime_language'] == 'rust' and len(row['ctx']['source_commit']) == 40 for row in peer.records)
          assert all('process-supervision/src/' in row['pathname'] for row in peer.records)
        pair.append({'implementation': implementation, 'result': result, 'messages': messages})
    assert pair[0]['result'] == pair[1]['result'], (name, pair)
    assert pair[0]['messages'] == pair[1]['messages'], (name, pair)
    results.append({'scenario': name, 'passed': True, 'results': pair})
    (args.output / 'summary.json').write_text(json.dumps({'passed': False, 'completed': len(results), 'results': results}, indent=2) + '\n')
    print('PASS:', name, flush=True)
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'scenarios': len(results), 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
  main()
