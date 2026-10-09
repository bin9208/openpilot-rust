# /// script
# requires-python = ">=3.12"
# dependencies = ["aiohttp"]
# ///
# Run with retained Python/PYTHONPATH from the evidence invocation; supply --binary, --launcher and --output.
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import select
import socket
import sys
import time

from carrot_server_git_status_cases import Fixtures
from carrot_server_tools_git_peer import Peer


def until(test, message: str) -> None:
  limit = time.monotonic() + 12
  while not test():
    if time.monotonic() >= limit:
      raise RuntimeError(message)
    time.sleep(0.01)


def live_descriptors(peer: Peer) -> list[int]:
  until(peer.descendant.exists, 'held Git descendant not observed')
  child = int(peer.descendant.read_text())
  leader = next(row['pid'] for row in peer.calls() if row['args'][0] == 'fetch')
  descriptors = [os.pidfd_open(pid) for pid in (leader, child)]
  if select.select(descriptors, [], [], 0)[0]:
    raise RuntimeError('held owned Git process already exited')
  (peer.root / 'live-pids.json').write_text(json.dumps({'leader': leader, 'descendant': child}))
  return descriptors


def exited(descriptors: list[int]) -> bool:
  result = len(select.select(descriptors, [], [], 0)[0]) == len(descriptors)
  for fd in descriptors:
    os.close(fd)
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--launcher', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  fixtures = Fixtures(args.output / 'repositories')
  source = [sys.executable, '-P', str(Path(__file__).with_name('carrot_server_tools_git_source.py'))]
  rows = []
  peers = []
  try:
    for scenario in ('initial_delay', 'active_cleanup'):
      repository = fixtures.clone(scenario)
      pair = [Peer(command, args.output / scenario / side, repository, fixtures.env, args.launcher,
                   composed=True, blocked=scenario == 'active_cleanup')
              for side, command in (('source', source), ('native', [str(args.binary)]))]
      peers.extend(pair)
      for peer in pair:
        if peer.calls() or peer.request('/plain.txt')['status'] != 200:
          raise AssertionError('production startup touched Git before its initial delay')
      if scenario == 'initial_delay':
        for peer in pair:
          until(lambda: bool(peer.calls()), 'default Git loop never started')
          first = peer.calls()[0]['time'] - peer.started
          if first < 7.8:
            raise AssertionError('default initial Git delay weakened')
          response = peer.request()
          rows.append({'scenario': scenario, 'side': peer.root.name, 'first_command_delay': first,
                       'status': response['status'], 'body_hex': response['body_hex']})
        if rows[-2]['body_hex'] != rows[-1]['body_hex'] or rows[-1]['status'] != 200:
          raise AssertionError('periodic result differs through real HTTP')
      descriptors = [live_descriptors(peer) for peer in pair] if scenario == 'active_cleanup' else []
      with ThreadPoolExecutor(max_workers=2) as pool:
        stops = list(pool.map(lambda peer: peer.stop(), pair))
      if descriptors:
        if not all(exited(fds) for fds in descriptors):
          raise AssertionError('active periodic cleanup left an owned Git process alive')
        if abs(stops[0]['elapsed'] - stops[1]['elapsed']) > 0.4:
          raise AssertionError('source/native active periodic cleanup timing differs')
      rows.append({'scenario': scenario, 'stops': stops, 'owned_processes_exited': bool(descriptors)})
      for peer in pair:
        peers.remove(peer)
    repository = fixtures.clone('listener_error')
    native = Peer([str(args.binary)], args.output / 'listener_error/native', repository, fixtures.env,
                  args.launcher, composed=True, blocked=True, expect_error=True)
    peers.append(native)
    descriptors = live_descriptors(native)
    started = time.monotonic()
    native.signal('error')
    if native.message() != {'limited': True}:
      raise AssertionError('owned descriptor-limit trigger was not ready')
    with socket.create_connection(('127.0.0.1', native.port), timeout=3):
      pass
    stopped = native.finish(started)
    peers.remove(native)
    if 'Too many open files' not in (stopped['last']['serve_error'] or '') or not exited(descriptors):
      raise AssertionError('real listener error did not cancel/await held Git work')
    rows.append({'scenario': 'listener_error', 'stop': stopped, 'owned_processes_exited': True})
    for scenario, options in (('validation_error', {'expect_error': True, 'invalid_web': True}),
                              ('inactive_constructor', {'inactive': True})):
      peer = Peer([str(args.binary)], args.output / scenario / 'native', repository, fixtures.env,
                  args.launcher, composed=True, **options)
      peers.append(peer)
      if scenario == 'inactive_constructor':
        if peer.request('/plain.txt')['status'] != 200:
          raise AssertionError('unrelated fixture could not serve an owned static file')
        time.sleep(8.1)
      stopped = peer.stop()
      peers.remove(peer)
      if peer.calls():
        raise AssertionError('inactive/failed startup fixture accessed Git')
      if scenario == 'validation_error' and 'web dir not found' not in stopped['last']['serve_error']:
        raise AssertionError('invalid startup did not report its actual validation error')
      rows.append({'scenario': scenario, 'git_commands': 0, 'stop': stopped})
    result = {'passed': True, 'production_lifecycle_scenarios': 5,
              'source_native_pairs': 2, 'native_listener_validation_inactive_controls': 3}
    (args.output / 'observations.json').write_text(json.dumps(rows, indent=2))
    (args.output / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
  finally:
    for peer in peers:
      if peer.process.poll() is None:
        peer.stop()


if __name__ == '__main__':
  main()
