#!/usr/bin/env python3
"""Hosted-only MLB comparison with authenticated primary functions and literal synthetic vectors."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import types
import urllib.request

from can_source import ROOT, load

PRIMARY = 'c3955753ae7258412814e2cdca3ee8af979e4679'
BASELINE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'
FILES = {
  'mqbcan.py': '67dd9da71aab09a1332072aef1b2228e3769a3ba',
  'mlbcan.py': 'dc3b717e9cde7265d2dff13a9372a673c01fe70d',
}


def functions(source: bytes, filename: str, names: set[str], scope: dict) -> None:
  # Compile original function ASTs, without rewriting a body or injecting an algorithm.
  selected = [node for node in ast.parse(source, filename).body
              if isinstance(node, ast.FunctionDef) and node.name in names]
  assert {node.name for node in selected} == names
  exec(compile(ast.Module(body=selected, type_ignores=[]), filename, 'exec'), scope)


def check(args, report: dict) -> None:
  primary = {}
  for name, blob in FILES.items():
    url = f'https://raw.githubusercontent.com/commaai/opendbc/{PRIMARY}/opendbc/car/volkswagen/{name}'
    with urllib.request.urlopen(url, timeout=60) as response:
      source = response.read()
    assert hashlib.sha1(f'blob {len(source)}\0'.encode() + source).hexdigest() == blob
    (args.evidence / f'primary-{name}').write_bytes(source)
    primary[name] = source
  report['primary_blobs'] = FILES
  scope = {}
  functions(primary['mqbcan.py'], 'primary/mqbcan.py', {'xor_checksum'}, scope)
  functions(primary['mlbcan.py'], 'primary/mlbcan.py', {'volkswagen_mlb_checksum'}, scope)
  old_helper = subprocess.check_output([
    'git', 'show', f'{BASELINE}:opendbc_repo/opendbc/car/volkswagen/mqbcan.py'], cwd=ROOT)
  old_caller = subprocess.check_output([
    'git', 'show', f'{BASELINE}:opendbc_repo/opendbc/car/volkswagen/mlbcan.py'], cwd=ROOT)
  old = {}
  functions(old_helper, 'baseline/mqbcan.py', {'xor_checksum'}, old)
  functions(old_caller, 'baseline/mlbcan.py', {'volkswagen_mlb_checksum'}, old)
  report['baseline_sha256'] = {name: hashlib.sha256(data).hexdigest()
                               for name, data in [('mqbcan.py', old_helper), ('mlbcan.py', old_caller)]}
  load()
  from opendbc.car.volkswagen.mlbcan import volkswagen_mlb_checksum
  from opendbc.car.volkswagen.mqbcan import xor_checksum
  fixture = ROOT / 'rust/crates/can/tests/data/mlb_497.json'
  corpus = json.loads(fixture.read_text())
  assert corpus['classification'] == 'synthetic_source_backed_literals_not_vehicle_captures'
  assert corpus['source_commit'] == PRIMARY and len(corpus['vectors']) == 10
  assert len({vector['address'] for vector in corpus['vectors']}) == 10
  report['fixture_sha256'] = hashlib.sha256(fixture.read_bytes()).hexdigest()
  request, expected, controls = [], [], []
  # Literal expectations are independent of both implementations; never derive them here.
  for vector in corpus['vectors']:
    assert len(vector['expected']) == len(corpus['payloads']) == 4
    for payload, literal in zip(corpus['payloads'], vector['expected'], strict=True):
      signal = types.SimpleNamespace(start_bit=payload['start_bit'])
      try:
        old['volkswagen_mlb_checksum'](vector['address'], signal, bytearray(payload['data']))
      except TypeError as error:
        assert 'positional arguments' in str(error)
        controls.append({'address': vector['address'], 'start_bit': signal.start_bit, 'error': str(error)})
      else:
        raise AssertionError('immutable baseline unexpectedly accepted the MLB four-argument call')
      for altered in (False, True):
        data = bytearray(payload['data'])
        if altered:
          data[signal.start_bit // 8] ^= 0xff
        original = bytes(data)
        assert scope['volkswagen_mlb_checksum'](vector['address'], signal, data) == literal
        assert volkswagen_mlb_checksum(vector['address'], signal, data) == literal
        assert bytes(data) == original
        request.append(dict(dbc='vw_mlb', address=vector['address'], start_bit=signal.start_bit, data=list(data)))
        expected.append(dict(checksum=literal, data=list(data), error=None))
  # The helper's three-argument default and the existing PQ XOR family stay unchanged.
  for payload, literal in zip(corpus['payloads'], [0x00, 0x21, 0x7f, 0xbf], strict=True):
    signal = types.SimpleNamespace(start_bit=payload['start_bit'])
    data = bytearray(payload['data'])
    assert scope['xor_checksum'](0, signal, data) == xor_checksum(0, signal, data) == literal
    request.append(dict(dbc='vw_pq', address=0, start_bit=signal.start_bit, data=list(data)))
    expected.append(dict(checksum=literal, data=list(data), error=None))
  (args.evidence / 'baseline-failures.json').write_text(json.dumps(controls, indent=2) + '\n')
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  target = args.evidence / 'native.json'
  child = subprocess.run([args.binary.resolve(), target.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  assert json.loads(target.read_text()) == expected
  report.update(status='pass', synthetic_mlb_cases=80, pq_default_cases=4, physical_capture_validation=False,
                binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest())
  report['current_sha256'] = {name: hashlib.sha256((ROOT / f'opendbc_repo/opendbc/car/volkswagen/{name}').read_bytes()).hexdigest()
                             for name in FILES}


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=False)
  report = {'primary_commit': PRIMARY, 'baseline_commit': BASELINE}
  try:
    check(args, report)
  finally:
    (args.evidence / 'partial.json').write_text(json.dumps(report, indent=2) + '\n')
  (args.evidence / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
