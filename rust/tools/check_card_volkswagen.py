#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: oracle Python rust/tools/check_card_volkswagen.py --binary PATH --numerics DIR --evidence DIR --op OP
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
from can_source import ROOT, load
from check_card_mazda import compare
from card_vehicle_source import normalize
from card_qa.volkswagen.source import trace
from card_qa.volkswagen.scenarios import cases


def decoded(value):
  from openpilot.cereal import car
  with car.CarParams.from_bytes(bytes(value['params'])) as cp:
    value['params'] = normalize(cp.to_dict())
  for key in ('initial_state', 'final_state'):
    if key in value:
      with car.CarState.from_bytes(bytes(value[key])) as state:
        value[key] = normalize(state.to_dict())
  for step in value.get('steps', []):
    with car.CarState.from_bytes(bytes(step['state'])) as state:
      step['state'] = normalize(state.to_dict())
    with car.CarControl.Actuators.from_bytes(bytes(step['actuators'])) as act:
      step['actuators'] = normalize(act.to_dict())
  return normalize(value)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--op', choices=['params', 'runtime', 'mqb_failure', 'seeded_controller', 'before_update', 'numeric_error'])
  parser.add_argument('--candidate')
  parser.add_argument('--name', action='append')
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / 'result.json').unlink(missing_ok=True)
  (args.evidence / 'failure.txt').unlink(missing_ok=True)
  load()
  import numpy
  assert numpy.__version__ == '2.5.3'
  captured = io.StringIO()
  with contextlib.redirect_stdout(captured), contextlib.redirect_stderr(captured):
    request = [case for case in cases(args.op, args.candidate) if (args.op is None or case['op'] == args.op) and (args.name is None or case['name'] in args.name)]
    assert request
    expected = normalize([trace(case) for case in request])
  (args.evidence / 'source.log').write_text(captured.getvalue() or 'no source diagnostics\n')
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  output = args.evidence.resolve() / 'native.json'
  output.unlink(missing_ok=True)
  command = [str(args.binary.resolve()), str(output), str(ROOT / 'opendbc_repo/opendbc/dbc'), str(ROOT / 'opendbc_repo/opendbc/car/torque_data'), str(args.numerics.resolve())]
  (args.evidence / 'command.json').write_text(json.dumps(command) + '\n')
  child = subprocess.run(command, input=json.dumps(request), text=True, capture_output=True, check=False)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  prints = iter(child.stdout.splitlines())
  for value in actual:
    if 'steps' in value:
      value['prints'] = [next(prints), *value['prints']]
  assert list(prints) == []
  errors = []
  for case, left, right in zip(request, expected, actual, strict=True):
    if 'error' in left:
      source_error, native_error = left.pop('error'), right.pop('error')
      if source_error is None:
        assert native_error is None
      elif case['op'] == 'mqb_failure':
        assert source_error == {'kind': 'NameError', 'message': "name 'np' is not defined"}
        assert native_error == {'kind': 'InheritedMqbNumpy', 'message': "inherited Volkswagen MQB first-update failure: name 'np' is not defined"}
      elif source_error['kind'] == 'AttributeError' and source_error['message'].startswith('module '):
        module, function = source_error['message'].split("'")[1::2]
        assert native_error == {'kind': 'SourceFunction', 'module': module.rsplit('.', 1)[-1], 'function': function}
      elif source_error['kind'] == 'AttributeError':
        assert native_error == {'kind': 'Stock', 'attribute': source_error['message'].split("'")[-2]}
      elif source_error == {'kind': 'TypeError', 'message': "'bool' object is not subscriptable"}:
        assert native_error == {'kind': 'Stock', 'attribute': 'eps_stock_values'}
      else:
        assert source_error['kind'] in ('ValueError', 'OverflowError')
        assert source_error['message'] in ('cannot convert float NaN to integer', 'cannot convert float infinity to integer')
        assert native_error['kind'] in ('Numeric', 'CanNumeric')
      if source_error:
        errors.append({'name': case['name'], 'source': source_error, 'native': native_error})
    left, right = decoded(left), decoded(right)
    try:
      compare(left, right, case['name'])
    except AssertionError as error:
      (args.evidence / 'failure.txt').write_text(str(error) + '\n')
      (args.evidence / 'failure-source.json').write_text(json.dumps(left) + '\n')
      (args.evidence / 'failure-native.json').write_text(json.dumps(right) + '\n')
      raise
  (args.evidence / 'errors.json').write_text(json.dumps(errors) + '\n')
  frames = sum(len(value.get('steps', [])) for value in expected)
  healthy = sum(step['state']['canValid'] for value in expected for step in value.get('steps', []))
  source = list((ROOT / 'opendbc_repo/opendbc/car/volkswagen').glob('*.py'))
  source += [ROOT / 'opendbc_repo/opendbc/car/interfaces.py', ROOT / 'opendbc_repo/opendbc/car/__init__.py']
  rust = list((ROOT / 'rust/crates/card/src/brands/volkswagen').glob('*.rs'))
  rust += list((ROOT / 'rust/crates/card/examples/volkswagen_trace').glob('*.rs')) + [ROOT / 'rust/crates/card/examples/volkswagen_trace.rs']
  rust += list((ROOT / 'rust/tools/card_qa/volkswagen').glob('*.py')) + [Path(__file__).resolve()]
  report = {'status': 'pass', 'cases': len(request), 'frames': frames, 'healthy_frames': healthy, 'invalid_frames': frames - healthy,
            'candidates': sorted({case['candidate'] for case in request}), 'op': args.op,
            'observable': 'exact full CarParams/CarState/Actuators, ordered CAN, parser and state/controller histories, copied messages, logs, packer counters, Params writes and failure phases',
            'runtime_python': False, 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'source_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in source},
            'rust_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in rust}}
  (args.evidence / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({key: report[key] for key in ('status', 'cases', 'frames', 'healthy_frames', 'invalid_frames', 'binary_sha256')}))


if __name__ == '__main__':
  main()
