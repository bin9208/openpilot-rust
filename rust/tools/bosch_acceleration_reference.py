#!/usr/bin/env python3
"""Exact source oracle for #607. No candidate output is used as an expectation."""
import ast
import dataclasses
import hashlib
import json
import math
import numbers
import platform
import struct
import sys
import types
from pathlib import Path


def source_estimator():
  bundle = json.loads((Path(__file__).parents[1] / 'upstream/bosch-acceleration-source-607.json').read_text())
  assert bundle['schema'] == 1
  assert bundle['source_commit'] == 'e8e152dbcfd9574a1791ed12026d1880e87689b3'
  module = types.ModuleType('_bosch_acceleration_source_607')
  sys.modules[module.__name__] = module
  scope = module.__dict__
  scope.update(math=math, Integral=numbers.Integral, dataclass=dataclasses.dataclass)
  verified = {}
  expected = {'opendbc_repo/opendbc/car/hyundai/radar_bosch.py': '8c4cf75bcf371980cedc77432d22950c5ca490eff8a300a97685f2f84419a2f6',
              'opendbc_repo/opendbc/car/radar_lead_filter.py': '55ab0ad700459c3c9b02fcfb8f9dcae54fd8626aee515d4b3e473422c70d5528'}
  assert bundle['files'].keys() == expected.keys()
  for path in sorted(bundle['files'], key=lambda path: path.endswith('/radar_bosch.py')):
    entry = bundle['files'][path]
    contents = entry['contents']
    digest = hashlib.sha256(contents.encode()).hexdigest()
    assert digest == entry['sha256'] == expected[path], f'original source hash mismatch: {path}'
    verified[path] = digest
    tree = ast.parse(contents, filename=path)
    if path.endswith('/radar_lead_filter.py'):
      exec(compile(tree, path, 'exec'), scope)
    else:
      wanted = {'BOSCH_STALE_NS', 'BOSCH_ALEAD_SAMPLE_PERIOD_S', 'BOSCH_ALEAD_STATE_MAX',
                'BoschLeadAccelerationState', 'BoschLeadAccelerationEstimator'}
      selected = []
      found = set()
      for node in tree.body:
        names = ({node.name} if isinstance(node, ast.ClassDef) else
                 {t.id for t in node.targets if isinstance(t, ast.Name)} if isinstance(node, ast.Assign) else set())
        if names & wanted:
          selected.append(node)
          found.update(names & wanted)
      assert found == wanted, f'original source definitions missing: {wanted - found}'
      # Nodes include the original decorators and expressions without rewriting.
      exec(compile(ast.Module(body=selected, type_ignores=[]), path, 'exec'), scope)
  return scope['BoschLeadAccelerationEstimator'], verified


def frozen(value):
  if isinstance(value, float) and not math.isfinite(value):
    return 'NaN' if math.isnan(value) else 'Infinity' if value > 0 else '-Infinity'
  if dataclasses.is_dataclass(value):
    return {field.name: frozen(getattr(value, field.name)) for field in dataclasses.fields(value)}
  if isinstance(value, dict):
    return {str(key): frozen(item) for key, item in value.items()}
  if isinstance(value, (list, tuple)):
    return [frozen(item) for item in value]
  if hasattr(value, '__dict__'):
    return frozen(vars(value))
  return value


def object_value(value):
  if isinstance(value, dict):
    return types.SimpleNamespace(**{key: object_value(item) for key, item in value.items()})
  if isinstance(value, list):
    return [object_value(item) for item in value]
  if isinstance(value, str) and value in ('NaN', 'Infinity', '-Infinity'):
    return float(value)
  return value


def differences(left, right, path=''):
  if isinstance(left, dict) and isinstance(right, dict):
    if left.keys() != right.keys():
      yield {'path': path, 'kind': 'keys'}
    for key in left.keys() & right.keys():
      yield from differences(left[key], right[key], f'{path}/{key}')
  elif isinstance(left, list) and isinstance(right, list):
    if len(left) != len(right):
      yield {'path': path, 'kind': 'length'}
    for index, (a, b) in enumerate(zip(left, right)):
      yield from differences(a, b, f'{path}/{index}')
  elif isinstance(left, (int, float)) and not isinstance(left, bool) and isinstance(right, (int, float)) and not isinstance(right, bool):
    a, b = struct.pack('>d', left).hex(), struct.pack('>d', right).hex()
    if a != b:
      yield {'path': path, 'kind': 'number_bits', 'frozen': left, 'source': right, 'frozen_bits': a, 'source_bits': b}
  elif left != right:
    yield {'path': path, 'kind': 'value', 'frozen': left, 'source': right}


def run(input_path, output_path, evidence_path):
  estimator_type, hashes = source_estimator()
  estimator = estimator_type()
  count, difference_count, changed_rows, examples = 0, 0, 0, []
  with Path(input_path).open() as rows, Path(output_path).open('w') as output:
    for index, line in enumerate(rows):
      row = json.loads(line)
      original = {key: row[key] for key in ('result', 'state', 'debug') if key in row}
      if row['op'] == 'new':
        estimator = estimator_type()
      elif row['op'] == 'reset':
        estimator.reset(row['reason'])
      elif row['op'] == 'update':
        count += 1
        ego = row['ego']
        if not isinstance(ego, (int, float)) or isinstance(ego, bool):
          ego = math.nan
        row['result'] = frozen(estimator.update(object_value(row['objects']), row['scan'], ego, set(row['live'])))
      else:
        raise ValueError(f'unknown original operation {row["op"]}')
      if 'debug' in row:
        row['debug'] = frozen(estimator.debug_snapshot(row['debug_now']))
      row['state'] = frozen(estimator)
      row_differences = 0
      for key, historical in original.items():
        for difference in differences(historical, row[key], f'{key}/{index}'):
          row_differences += 1
          if len(examples) < 20:
            examples.append(difference)
      difference_count += row_differences
      changed_rows += bool(row_differences)
      output.write(json.dumps(row, allow_nan=False, separators=(',', ':')) + '\n')
  assert count == 2544, f'original input coverage changed: {count}'
  libraries = {}
  maps = Path('/proc/self/maps')
  if maps.exists():
    for line in maps.read_text().splitlines():
      path = line.split()[-1]
      if path.startswith('/') and any(name in Path(path).name for name in ('libm.', 'libm-', 'libc.', 'libc-')):
        libraries[path] = hashlib.sha256(Path(path).read_bytes()).hexdigest()
  evidence = dict(source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', source_files=hashes,
                  python=sys.version, machine=platform.machine(), libc=platform.libc_ver(), libraries=libraries,
                  updates=count, frozen_changed_rows=changed_rows, frozen_different_fields=difference_count, examples=examples,
                  comparison='Every result/debug/full-state field; floating point bits; no tolerance or candidate-derived expectations')
  Path(evidence_path).write_text(json.dumps(evidence, indent=2) + '\n')
  print(json.dumps(evidence), file=sys.stderr)


if __name__ == '__main__':
  run(*sys.argv[1:])
