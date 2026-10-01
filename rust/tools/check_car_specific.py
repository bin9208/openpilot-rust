#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp", "pyzmq"]
# ///
# ─── How to run ───
# PYTHONPATH=. python rust/tools/check_car_specific.py --binary <car_specific> --binding <params_pyx.so> --output <evidence>
# ──────────────────
from __future__ import annotations

import argparse
import hashlib
from functools import partial
import json
import os
from pathlib import Path
import subprocess
import tempfile

from openpilot.cereal import car
from car_specific_cases import cases, cp, state
from car_specific_oracle import ROOT, SOURCE, SourceTrace, TraceParams, load, snapshot, wire
from original_params_binding import load as load_binding


def configure(root, request):
  path = root / 'fixture' / request['key']
  if path.exists():
    if path.is_dir():
      path.rmdir()
    else:
      path.unlink()
  if request['directory']:
    path.mkdir()
  elif request['bytes'] is not None:
    path.write_bytes(bytes(request['bytes']))


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  os.environ['OPENPILOT_PREFIX'] = 'fixture'
  rows = cases()
  trace = SourceTrace()
  seen_events = set()
  requests, expected = [], []
  with tempfile.TemporaryDirectory(prefix='selfdrive-car-specific-') as temporary:
    temporary = Path(temporary)
    binding, _ = load_binding(args.binding.resolve(), 'ipc://' + str(temporary / 'source-log'), temporary / 'logs')
    params = TraceParams(binding.Params(str(temporary / 'source')))
    oracle = load(params)
    machine = None
    previous = state()
    for row in rows:
      params.effects = []
      operation = row['operation']
      request = {key: value for key, value in row.items() if key != 'scenario'}
      events = []
      if operation == 'init':
        raw, message = wire(car.CarParams, row['cp'])
        request['cp'] = list(raw)
        machine = trace.run(lambda message=message: oracle.cls(message))
        previous = state()
      elif operation in ('update', 'common'):
        current_raw, current = wire(car.CarState, row['current'])
        previous_raw, prev = wire(car.CarState, previous)
        control_raw, control = wire(car.CarControl, row['control'])
        request.update(current=list(current_raw), previous=list(previous_raw), control=list(control_raw))
        if operation == 'update':
          events = trace.run(partial(machine.update, current, prev, control)).names.copy()
        else:
          options = {key: row[key] for key in ('pcm_enable', 'allow_enable', 'allow_button_cancel')}
          events = trace.run(partial(machine.create_common_events, current, prev, **options)).names.copy()
        previous = row['current']
      elif operation == 'update_params':
        trace.run(machine.update_params)
      elif operation == 'parameter':
        configure(temporary / 'source', request)
      else:
        raise AssertionError(operation)
      seen_events.update(events)
      requests.append(request)
      expected.append({'events': events, 'state': snapshot(machine), 'effects': params.effects.copy()})
    payload = ''.join(json.dumps(row) + '\n' for row in requests)
    (args.output / 'input.jsonl').write_text(payload)
    (args.output / 'source.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in expected))
    (args.output / 'scenarios.jsonl').write_text(''.join(json.dumps({'step': i, 'name': row['scenario']}) + '\n' for i, row in enumerate(rows)))
    command = [str(args.binary.resolve()), str(temporary / 'native'), 'fixture', 'ipc://' + str(temporary / 'native-log')]
    result = subprocess.run(command, input=payload, capture_output=True, text=True, timeout=120)
    (args.output / 'native.jsonl').write_text(result.stdout)
    (args.output / 'native.stderr').write_text(result.stderr or 'no native stderr\n')
    assert result.returncode == 0, result.stderr
    actual = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(actual) == len(expected), (len(actual), len(expected))
    for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
      assert native == source, (index, rows[index], native, source)
    conditions = []
    for start, end, body in oracle.conditions:
      targets = {destination for origin, destination in trace.arcs if start <= origin <= end and not start <= destination <= end}
      conditions.append({'line': start, 'true': body in targets, 'false': bool(targets - {body})})
    assert all(row['true'] and row['false'] for row in conditions), ('uncovered source conditions', conditions)
    missing = oracle.add_lines - trace.lines
    assert not missing, ('source event branches not exercised', sorted(missing))
    failures = invalid_inputs(args.binary.resolve(), temporary, requests[0], args.output)
    for key in ('MuteSeatbelt', 'MuteDoor', 'ExperimentalModeConfirmed', 'ExperimentalMode', 'DoShutdown'):
      source_path = temporary / 'source' / 'fixture' / key
      native_path = temporary / 'native' / 'fixture' / key
      source_bytes = source_path.read_bytes() if source_path.is_file() else None
      native_bytes = native_path.read_bytes() if native_path.is_file() else None
      assert source_bytes == native_bytes, (key, source_bytes, native_bytes)
    keys = ('MuteSeatbelt', 'MuteDoor', 'ExperimentalModeConfirmed', 'ExperimentalMode', 'DoShutdown')
    final_params = {key: (temporary / 'source' / 'fixture' / key).read_bytes().hex()
                    if (temporary / 'source' / 'fixture' / key).is_file() else None for key in keys}
    (args.output / 'params-final.json').write_text(json.dumps(final_params, indent=2) + '\n')
  paths = [SOURCE, ROOT / 'rust/crates/selfdrived/data/alerts.json', ROOT / 'openpilot/selfdrive/selfdrived/events.py',
           ROOT / 'opendbc_repo/opendbc/car/car.capnp', ROOT / 'openpilot/cereal/log.capnp',
           ROOT / 'opendbc/car/__init__.py', ROOT / 'opendbc/car/interfaces.py', ROOT / 'opendbc/car/common/conversions.py',
           ROOT / 'opendbc/car/volkswagen/values.py', ROOT / 'opendbc/car/hyundai/interface.py', ROOT / 'opendbc/car/hyundai/carstate.py',
           ROOT / 'openpilot/selfdrive/carrot/bluetooth/model.py', ROOT / 'openpilot/common/params_pyx.pyx']
  report = {'result': 'PASS', 'steps': len(rows), 'scenarios': len({row['scenario'] for row in rows}),
            'source_conditions': len(conditions), 'covered_source_condition_outcomes': sum(row['true'] + row['false'] for row in conditions),
            'source_event_branches': len(oracle.add_lines), 'covered_source_event_branches': len(oracle.add_lines & trace.lines),
            'events': sorted(seen_events), 'constants': oracle.constants, 'invalid_inputs': failures,
            'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths},
            'owned_sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in
                              [ROOT / 'rust/crates/selfdrived/src/car_specific.rs',
                               *sorted((ROOT / 'rust/crates/selfdrived/src/car_specific').glob('*.rs')),
                               ROOT / 'rust/crates/selfdrived/examples/car_specific.rs',
                               *sorted((ROOT / 'rust/tools').glob('*car_specific*.py'))]},
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
            'binding_sha256': hashlib.sha256(args.binding.read_bytes()).hexdigest(),
            'input_sha256': hashlib.sha256(payload.encode()).hexdigest(),
            'scope': 'CarSpecificEvents only; MockCarState GPS selection separate; host evidence, no device/performance claims'}
  (args.output / 'source-coverage.json').write_text(json.dumps({'lines': sorted(trace.lines), 'arcs': sorted(trace.arcs),
                                                            'event_add_lines': sorted(oracle.add_lines), 'conditions': conditions}, indent=2) + '\n')
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps(report))


def invalid_inputs(binary, temporary, init, output):
  cp_raw, _ = wire(car.CarParams, cp(networkLocation=65535))
  gear_raw, _ = wire(car.CarState, state(gearShifter=65535))
  button_raw, _ = wire(car.CarState, state(buttonEvents=[{'type': 65535, 'pressed': True}]))
  valid_raw, _ = wire(car.CarState, state())
  control_raw, _ = wire(car.CarControl, {'enabled': False, 'actuators': {'accel': 0.}})
  brand_raw, _ = wire(car.CarParams, cp())
  offset = brand_raw.index(b'other\0')
  brand_raw = brand_raw[:offset] + b'\xff' + brand_raw[offset + 1:]
  valid = {'operation': 'update', 'current': list(valid_raw), 'previous': list(valid_raw), 'control': list(control_raw)}
  requests = {'network_enum': {'operation': 'init', 'cp': list(cp_raw)},
              'invalid_brand_utf8': {'operation': 'init', 'cp': list(brand_raw)},
              'gear_enum': {**valid, 'current': list(gear_raw)}, 'button_enum': {**valid, 'current': list(button_raw)},
              'previous_gear_enum': {**valid, 'previous': list(gear_raw)}, 'truncated_state': {**valid, 'current': [0]},
              'truncated_control': {**valid, 'control': [0]}, 'truncated_cp': {'operation': 'init', 'cp': [0]},
              'uninitialized': valid, 'unknown_operation': {'operation': 'unknown'}, 'missing_input': {'operation': 'update'}}
  results = []
  for name, request in requests.items():
    prefix = [] if name in ('network_enum', 'truncated_cp', 'invalid_brand_utf8', 'uninitialized') else [init]
    payload = ''.join(json.dumps(row) + '\n' for row in [*prefix, request])
    (output / f'invalid-{name}.input.jsonl').write_text(payload)
    run = subprocess.run([str(binary), str(temporary / ('invalid-' + name)), 'fixture', 'ipc://' + str(temporary / 'invalid-log')],
                         input=payload, capture_output=True, text=True, timeout=10)
    (output / f'invalid-{name}.stderr').write_text(run.stderr)
    (output / f'invalid-{name}.stdout').write_text(run.stdout or 'no successful output\n')
    assert run.returncode != 0 and run.stderr, (name, run.returncode, run.stderr)
    if name.endswith('_enum'):
      assert 'Enum(NotInSchema(65535))' in run.stderr, (name, run.stderr)
    if name == 'invalid_brand_utf8':
      assert 'BrandText(' in run.stderr, run.stderr
    assert len(run.stdout.splitlines()) == len(prefix), (name, run.stdout)
    results.append({'name': name, 'returncode': run.returncode})
  return results


if __name__ == '__main__':
  main()
