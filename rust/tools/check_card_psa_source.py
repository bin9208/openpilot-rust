#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: verified oracle Python rust/tools/check_card_psa_source.py --evidence DIR --binary PATH
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import traceback

from can_source import ROOT, load
from card_qa.mazda.source import Settings


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  load()
  import numpy
  from opendbc.car import Bus, interfaces, structs
  from opendbc.car.psa.carstate import CarState
  from opendbc.car.psa.interface import CarInterface
  from opendbc.car.psa.values import CAR

  assert numpy.__version__ == '2.5.3'
  settings = Settings({})
  interfaces.Params = lambda: settings
  candidate = 'PSA_PEUGEOT_208'
  assert [str(c) for c in CAR] == [candidate]
  cp = structs.CarParams.new_message(carFingerprint=candidate)
  state = CarState(cp)
  calls = [
    ('params', lambda: CarInterface.get_params(candidate, {bus: {} for bus in range(8)}, [], False, True, False), KeyError, repr(candidate)),
    ('constructor_candidate_only', lambda: CarInterface(cp), FileNotFoundError, 'psa_aee2010_r3.dbc'),
    (
      'state_candidate_only',
      lambda: state.update({Bus.main: object(), Bus.adas: object(), Bus.cam: object()}),
      AttributeError,
      "'CarState' object has no attribute 'parse_wheel_speeds'",
    ),
  ]
  output = io.StringIO()
  failures = []
  for name, call, kind, message in calls:
    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
      try:
        call()
      except kind as error:
        assert message in str(error)
        traceback.print_exc()
        failures.append({'scenario': name, 'kind': type(error).__name__, 'message': str(error)})
      else:
        raise AssertionError(f'{name} unexpectedly succeeded')
  assert not settings.writes
  assert not hasattr(state, 'parse_wheel_speeds')
  assert not (ROOT / 'opendbc_repo/opendbc/dbc/psa_aee2010_r3.dbc').exists()
  files = list((ROOT / 'opendbc_repo/opendbc/car/psa').glob('*.py'))
  files += [ROOT / 'opendbc_repo/opendbc/car/interfaces.py', ROOT / 'rust/crates/card/data/vehicle.json']
  files += list((ROOT / 'opendbc_repo/opendbc/car/torque_data').glob('*.toml'))
  tracked = subprocess.check_output(['git', 'ls-files', 'opendbc_repo/opendbc/dbc'], cwd=ROOT, text=True).splitlines()
  assert not any(Path(path).stem == 'psa_aee2010_r3' for path in tracked)
  report = {
    'status': 'source_blocked',
    'candidate': candidate,
    'failures': failures,
    'params_writes': settings.writes,
    'constructor_scope': 'candidate-only to isolate later failures; normal get_params already fails',
    'runtime_gate_complete': False,
    'dbc_absent_tracked_and_local': True,
    'base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
    'numpy': {'version': numpy.__version__, 'path': numpy.__file__},
    'source_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(files))},
  }
  if args.binary is not None:
    request = [
      {'op': op, 'candidate': candidate, 'alpha_long': False, 'fingerprints': [], 'firmware': [], 'settings': {}}
      for op in ('params', 'constructor', 'state')
    ]
    dbc = ROOT / 'opendbc_repo/opendbc/dbc'
    expected = [
      {'kind': 'MissingTorque', 'candidate': candidate},
      {'kind': 'MissingDbc', 'path': str(dbc / 'psa_aee2010_r3.dbc')},
      {'kind': 'MissingMethod', 'method': 'parse_wheel_speeds'},
    ]
    native_path = args.evidence.resolve() / 'native.json'
    command = [str(args.binary.resolve()), str(native_path), str(dbc)]
    (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
    (args.evidence / 'command.json').write_text(json.dumps(command) + '\n')
    native_path.unlink(missing_ok=True)
    child = subprocess.run(command, input=json.dumps(request), capture_output=True, text=True, check=False)
    (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
    child.check_returncode()
    actual = json.loads(native_path.read_text())
    assert actual == [{'failure': failure, 'writes': [], 'prints': []} for failure in expected]
    assert child.stdout == '' and child.stderr == ''
    report['boundary_gate'] = 'pass'
    report['boundary_cases'] = len(request)
    report['binary_sha256'] = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    report['rust_sha256'] = {
      str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
      for path in (ROOT / 'rust/crates/card/src/brands/psa.rs', ROOT / 'rust/crates/card/examples/psa_boundary.rs',
                   ROOT / 'rust/crates/card/tests/psa_source.rs', ROOT / 'rust/tools/check_card_psa_source.py')
    }
  (args.evidence / 'source-blockers.json').write_text(json.dumps(report, indent=2) + '\n')
  (args.evidence / 'source-blockers-tracebacks.log').write_text(output.getvalue())
  print(json.dumps(report))


if __name__ == '__main__':
  main()
