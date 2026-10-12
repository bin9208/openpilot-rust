from __future__ import annotations

import json
from pathlib import Path
import socket
import time

from carrot_server_heartbeat_driver import Composition, Driver, until
from carrot_server_heartbeat_lifecycle import http_observable
from carrot_server_heartbeat_peer import Peer, Response, response


def scenario(side: str, name: str, binary: Path, root: Path):
  held = name in ('active-cancel', 'accept-error')
  wire = response(b'failed', 404) if name == 'http-error' else response(b'ready')
  peer = Peer((Response(wire.wire, hold=held),))
  constructor = {'inactive-new': 'new', 'inactive-git': 'with_git_status'}.get(name, 'runtime')
  driver = Driver(side, binary, root, peer, has_params=name != 'unavailable',
                  params=(('Version', b'app-version'), ('GithubUsername', b'app-owner'), ('IsOnroad', b'1')),
                  composition=Composition(constructor, name == 'startup-error'))
  row = dict(scenario=name, side=side)
  try:
    if name != 'startup-error':
      if name not in ('unavailable', 'inactive-new', 'inactive-git'):
        until(peer.received.is_set)
        if not held:
          until(lambda: driver.command('status').get('ts') == 1700000001.25)
      row['before_cleanup'] = driver.command('status')
      row['http'] = [driver.http(method) for method in ('GET', 'HEAD', 'POST')]
    if name == 'accept-error':
      row['limit'] = driver.command('limit_accept')
      with socket.create_connection(('127.0.0.1', driver.port), timeout=3):
        pass
    started = time.monotonic()
    row['cleanup'] = driver.command('server_cleanup')
    row['cleanup_seconds'] = time.monotonic()-started
    row['child_alive_after_cleanup'] = driver.process.poll() is None
    row['after_cleanup'] = driver.command('status')
    if held:
      row['worker_incomplete_after_cleanup'] = not peer.completed
      peer.release.set()
      until(lambda: len(peer.completed) == 1)
      row['after_worker_response'] = driver.command('status')
    if side == 'source':
      row['provider_options'] = driver.command('observations')
    row['requests'] = peer.rows
    row['completed'] = peer.completed
    row['recipient_errors'] = peer.errors
  finally:
    peer.release.set()
    started = time.monotonic()
    driver.close()
    row['process_exit_seconds'] = time.monotonic()-started
    row['exit_code'] = driver.process.returncode
    peer.close()
  assert row['exit_code'] == 0 and row['child_alive_after_cleanup']
  assert row['cleanup_seconds'] < 0.5, row
  if held:
    assert row['worker_incomplete_after_cleanup']
    assert row['after_cleanup'] == row['after_worker_response'] == row['before_cleanup']
    assert row['completed'] == [0]
  match name:
    case 'unavailable' | 'inactive-new' | 'inactive-git' | 'startup-error':
      assert not row['requests'] and row['after_cleanup'] == dict(ok=None, msg='not yet', ts=0)
    case 'http-error':
      assert row['after_cleanup']['ok'] is False and row['after_cleanup']['msg'] == 'HTTPError 404: failed'
    case 'success':
      assert row['after_cleanup'] == dict(ok=True, msg='ready', ts=1700000001.25, local_ip='127.0.0.3')
    case 'active-cancel' | 'accept-error':
      assert row['after_cleanup'] == dict(ok=None, msg='not yet', ts=0)
    case name:
      raise ValueError(name)
  expected_error = {'startup-error': 'web dir not found', 'accept-error': 'Too many open files'}.get(name)
  assert (expected_error in (row['cleanup']['serve_error'] or '')) if expected_error else row['cleanup']['serve_error'] is None
  assert row['cleanup']['active'] == (name not in ('unavailable', 'inactive-new', 'inactive-git'))
  (root / 'receipt.json').write_text(json.dumps(row, indent=2)+'\n')
  return row


def paired(source, native):
  keys = ('before_cleanup', 'cleanup', 'after_cleanup', 'worker_incomplete_after_cleanup', 'after_worker_response')
  if any(source.get(key) != native.get(key) for key in keys):
    return False
  if [http_observable(row) for row in source['http']] != [http_observable(row) for row in native['http']]:
    return False
  return [(row['request_line'], row['body']) for row in source['requests']] == [(row['request_line'], row['body']) for row in native['requests']]


def run(binary: Path, root: Path, selected: tuple[str, ...] = ()):
  results = []
  for name in ('unavailable', 'success', 'http-error', 'active-cancel', 'inactive-new', 'inactive-git', 'startup-error', 'accept-error'):
    if selected and name not in selected:
      continue
    source = scenario('source', name, binary, root / name / 'source') if name in ('unavailable', 'success', 'http-error', 'active-cancel') else None
    native = scenario('native', name, binary, root / name / 'native')
    passed = paired(source, native) if source else True
    print('Application', name, 'source/native' if source else 'native-only', 'PASS' if passed else 'FAIL', 'cleanup', native['cleanup_seconds'], 'process_exit', native['process_exit_seconds'], flush=True)
    results.append(dict(case=name, source=source, native=native, passed=passed))
  (root / 'result.json').write_text(json.dumps(results, indent=2)+'\n')
  return results
