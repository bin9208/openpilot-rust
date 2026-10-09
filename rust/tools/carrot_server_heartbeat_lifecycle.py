from __future__ import annotations

import json
from pathlib import Path
import time

from carrot_server_heartbeat_driver import Driver, until
from carrot_server_heartbeat_peer import Peer, Response, response


def http_observable(receipt):
  headers = {name.lower(): value for name, value in receipt['headers']}
  return receipt['status'], receipt['body'], {name: headers.get(name) for name in ('content-type', 'content-length', 'allow')}


def scenario(side: str, name: str, binary: Path | None, root: Path):
  long_message = ('한😀x'*400).encode()
  reply = Response(response().wire, hold=True) if name == 'active-cancel' else response(long_message if name == 'result-status' else b'tick')
  peer = Peer((reply,))
  driver = Driver(side, binary, root, peer, has_params=name != 'unavailable', times=(1000.9, 1001.25, 1002.9, 1003.25))
  observations = {}
  try:
    observations['initial'] = driver.command('status')
    observations['http_initial'] = [driver.http(method) for method in ('GET', 'HEAD', 'POST')]
    if name == 'ip-fallback':
      observations['ip'] = [driver.command('ip', route_peer=f'{peer.address[0]}:{peer.address[1]}', route_ok=route, hostname=hostname) for route, hostname in ((True, None), (False, 'localhost'), (False, None))]
    else:
      driver.command('start')
      if name == 'unavailable':
        until(lambda: driver.command('status').get('msg') == 'Params not available')
      else:
        until(peer.received.is_set)
        if name != 'active-cancel':
          until(lambda: driver.command('status').get('ts') == 1001.25)
      if name == 'cadence':
        until(lambda: len(peer.rows) >= 2, timeout=32)
        until(lambda: driver.command('status').get('ts') == 1003.25)
        observations['cadence_seconds'] = peer.rows[1]['received']-peer.rows[0]['received']
      started = time.monotonic()
      observations['stop'] = driver.command('stop')
      observations['stop_seconds'] = time.monotonic()-started
      observations['stopped'] = driver.command('status')
      if name == 'active-cancel':
        observations['worker_incomplete_at_stop'] = not peer.completed
        peer.release.set()
        until(lambda: len(peer.completed) == 1)
        observations['after_worker_response'] = driver.command('status')
      observations['http_stopped'] = [driver.http(method) for method in ('GET', 'HEAD')]
    observations['requests'] = peer.rows
    observations['errors'] = peer.errors
    observations['completed'] = peer.completed
    if side == 'source':
      observations['provider_options'] = driver.command('observations')
  finally:
    peer.release.set()
    driver.close()
    peer.close()
  (root / 'receipt.json').write_text(json.dumps(observations, indent=2)+'\n')
  return observations


def equivalent(source, native, name: str) -> bool:
  keys = ('initial', 'ip', 'stop', 'stopped', 'worker_incomplete_at_stop', 'after_worker_response')
  if any(source.get(key) != native.get(key) for key in keys):
    return False
  for key in ('http_initial', 'http_stopped'):
    if [http_observable(row) for row in source.get(key, [])] != [http_observable(row) for row in native.get(key, [])]:
      return False
  if [row['body'] for row in source['requests']] != [row['body'] for row in native['requests']]:
    return False
  if name == 'cadence' and not all(29.9 <= side['cadence_seconds'] < 32 for side in (source, native)):
    return False
  return name != 'active-cancel' or all(side['stop_seconds'] < 0.5 for side in (source, native))


def run(binary: Path | None, root: Path, selected: tuple[str, ...] = ()):
  results = []
  for name in ('result-status', 'unavailable', 'active-cancel', 'ip-fallback', 'cadence'):
    if selected and name not in selected:
      continue
    source = scenario('source', name, binary, root / name / 'source')
    native = scenario('native', name, binary, root / name / 'native') if binary else None
    passed = equivalent(source, native, name) if native else None
    print(name, 'source stop', source.get('stop'), 'native stop', native.get('stop') if native else None, 'equal', passed, flush=True)
    results.append(dict(case=name, source=source, native=native, passed=passed))
  (root / 'result.json').write_text(json.dumps(results, indent=2)+'\n')
  return results
