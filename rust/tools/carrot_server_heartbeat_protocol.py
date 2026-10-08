from __future__ import annotations

import json
from pathlib import Path
import time

from carrot_server_heartbeat_cases import Case, cases
from carrot_server_heartbeat_driver import Driver, ROOT
from carrot_server_heartbeat_peer import Peer


def run_case(side: str, case: Case, binary: Path | None, root: Path):
  certificates = ROOT / 'rust/vendor/ureq/src/unversioned/transport/testdata'
  certificate = (certificates / 'cert.pem', certificates / 'key.pem') if case.tls else None
  peer = Peer(case.responses, certificate)
  driver = Driver(side, binary, root, peer, params=case.params, times=(case.timestamp,))
  try:
    started = time.monotonic()
    result = driver.command('register')
    elapsed = time.monotonic()-started
    verified = driver.command('verified_tls') if case.tls else None
    observations = driver.command('observations') if side == 'source' else None
    receipt = dict(result=result, elapsed=elapsed, verified_tls=verified, requests=peer.rows, completed=peer.completed, errors=peer.errors, observations=observations)
  finally:
    peer.release.set()
    driver.close()
    peer.close()
  (root / 'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
  return receipt


def compare(source, native):
  requests = lambda rows: [(row['request_line'], row['body'], row['headers'].get('content-type')) for row in rows]
  return source['result'] == native['result'] and source['verified_tls'] == native['verified_tls'] and requests(source['requests']) == requests(native['requests'])


def run(binary: Path | None, root: Path, selected: tuple[str, ...] = ()):
  results = []
  for case in cases():
    if selected and case.name not in selected:
      continue
    source = run_case('source', case, binary, root / case.name / 'source')
    native = run_case('native', case, binary, root / case.name / 'native') if binary else None
    passed = compare(source, native) if native else None
    print(case.name, 'source', json.dumps(source['result']), 'native', json.dumps(native['result'] if native else None), 'equal', passed, flush=True)
    results.append(dict(case=case.name, source=source, native=native, passed=passed))
  (root / 'result.json').write_text(json.dumps(results, indent=2)+'\n')
  return results
