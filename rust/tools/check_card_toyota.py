#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0", "pycryptodome==3.23.0"]
# ///
# How to run: oracle Python rust/tools/check_card_toyota.py --binary PATH --numerics DIR --dbc DIR --evidence DIR
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT, load
from check_card_rivian import decoded
from check_card_mazda import compare
from card_qa.toyota.source import trace
from card_qa.toyota.scenarios import cases


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--dbc', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--op', choices=['params', 'runtime', 'before_update', 'numeric_error'])
  parser.add_argument('--candidate')
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / 'result.json').unlink(missing_ok=True)
  (args.evidence / 'failure.txt').unlink(missing_ok=True)
  load()
  import numpy
  import Crypto
  assert Crypto.__version__ == '3.23.0'
  import opendbc.can.dbc as source_dbc
  assert numpy.__version__ == '2.5.3'
  dbc = args.dbc.resolve()
  source_dbc.DBC_PATH = str(dbc)
  source_dbc.DBC.cache_clear()
  request = [case for case in cases() if (args.op is None or case['op'] == args.op) and (args.candidate is None or case['candidate'] == args.candidate)]
  assert request
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured), contextlib.redirect_stderr(captured):
    from card_vehicle_source import normalize
    expected = normalize([trace(case) for case in request])
  (args.evidence / 'source.log').write_text(captured.getvalue() or 'no source diagnostics\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  output = args.evidence.resolve() / 'native.json'
  output.unlink(missing_ok=True)
  command = [str(args.binary.resolve()), str(output), str(dbc), str(ROOT / 'opendbc_repo/opendbc/car/torque_data'), str(args.numerics.resolve())]
  (args.evidence / 'command.json').write_text(json.dumps(command) + '\n')
  child = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  prints = iter(child.stdout.splitlines())
  for case in actual:
    if 'steps' in case: case['prints'] = [next(prints), *case['prints']]
  assert list(prints) == []
  left, right = [decoded(value) for value in expected], [decoded(value) for value in actual]
  errors = []
  for case, source, native in zip(request, left, right, strict=True):
    if case['op'] == 'before_update':
      source_error = source.pop('pre_update_error')
      native_error = native.pop('pre_update_error')
      if source_error['kind'] == 'TypeError':
        assert source_error['message'] == "'NoneType' object is not subscriptable"
        attribute = 'secoc_synchronization'
      else:
        assert source_error['kind'] == 'AttributeError'
        attribute = source_error['message'].split("'")[-2]
      assert native_error == {'kind': 'Stock', 'message': f'Toyota stock message unavailable before state update: {attribute}'}
      errors.append({'name': case['name'], 'source': source_error, 'native': native_error})
    if case['op'] == 'numeric_error':
      source_error = source.pop('pre_update_error')
      native_error = native.pop('pre_update_error')
      if case['name'].startswith('setting-'):
        assert source_error['message'] == 'stoi' and source_error['kind'] in ('ValueError', 'OverflowError')
        assert native_error == {'kind': 'SettingInteger', 'key': case['error_key']}
      else:
        assert source_error['kind'] in ('ValueError', 'OverflowError')
        assert source_error['message'] in ('cannot convert float NaN to integer', 'cannot convert float infinity to integer', 'math domain error')
        assert native_error == {'kind': 'CanNumeric' if case['name'].startswith('accel-') else 'Numeric'}
      errors.append({'name': case['name'], 'source': source_error, 'native': native_error})
  (args.evidence / 'source-errors.json').write_text(json.dumps(errors) + '\n')
  (args.evidence / 'source-fields.json').write_text(json.dumps(left) + '\n')
  (args.evidence / 'native-fields.json').write_text(json.dumps(right) + '\n')
  try:
    compare(left, right)
  except AssertionError as error:
    (args.evidence / 'failure.txt').write_text(str(error) + '\n')
    raise
  files = list((ROOT / 'opendbc_repo/opendbc/car/toyota').glob('*.py'))
  files += [ROOT / 'opendbc_repo/opendbc/car/interfaces.py', ROOT / 'opendbc_repo/opendbc/car/__init__.py']
  files += [ROOT / 'opendbc_repo/opendbc/car/secoc.py', ROOT / 'opendbc_repo/opendbc/car/common/pid.py', ROOT / 'opendbc_repo/opendbc/car/common/filter_simple.py']
  rust = list((ROOT / 'rust/crates/card/src/brands/toyota').glob('*.rs'))
  rust += [ROOT / 'rust/crates/card/examples/toyota_trace.rs', Path(__file__).resolve(), ROOT / 'rust/crates/card/Cargo.toml', ROOT / 'rust/Cargo.lock']
  rust += list((ROOT / 'rust/crates/card/examples/toyota_trace').glob('*.rs'))
  rust += list((ROOT / 'rust/tools/card_qa/toyota').glob('*.py'))
  report = {'status': 'pass', 'cases': len(request), 'frames': sum(len(case['steps']) for case in request if case['op'] == 'runtime'),
            'before_update_failures': sum(case['op'] == 'before_update' for case in request),
            'numeric_failures': sum(case['op'] == 'numeric_error' for case in request),
            'candidates': sorted({case['candidate'] for case in request}), 'runtime_python': False,
            'observable': 'full CarParams/CarState/Actuators, exact ordered CAN, state/controller histories, stock messages, parser logs, packer counters, Common lifecycle and Params writes',
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files},
            'rust_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in rust},
            'dbc_sha256': {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in dbc.glob('toyota*.dbc')}}
  (args.evidence / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
