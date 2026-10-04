#!/usr/bin/env python3
import argparse
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

import numpy as np


def commands(kind: str):
  lateral = kind == 'lateral'
  horizon, states = (32, 4) if lateral else (12, 3)
  sequence = []
  for speed in (5.0, 20.0, 35.0):
    if speed != 20.0:
      sequence.append({'op': 'reset'})
    initial = [0.0, 0.0, 0.001, 0.0] if lateral else [0.0, speed, 0.0]
    for stage in range(horizon + 1):
      cost_count = (3 if stage == horizon else 5) if lateral else (5 if stage == horizon else 6)
      weights = np.diag(([2.0, 0.11, 0.0, 0.04, 700.0] if lateral else [5.0, 0.0, 0.0, 0.0, 200.0, 5.0])[:cost_count])
      if lateral:
        weights[0, 1] = weights[1, 0] = 0.005
      parameters = [speed, 1.5] if lateral else [-4.0, 2.0, 100.0, 0.0, 1.45, 0.8, 2.4, 6.0]
      reference = [0.01 * stage, 0.002 * stage, 0.0, 0.0, 0.0][:cost_count] if lateral else [0.0] * cost_count
      for field, values in [('state', initial), ('parameters', parameters), ('weights', weights.flatten(order='F').tolist()), ('reference', reference)]:
        sequence.append({'op': 'set', 'stage': stage, 'field': field, 'values': values})
      if not lateral and stage < horizon:
        sequence.append({'op': 'set', 'stage': stage, 'field': 'lower_slack', 'values': [1e6, 1e6, 1e6, 100.0]})
    for field in ('lower_bound', 'upper_bound'):
      sequence.append({'op': 'set', 'stage': 0, 'field': field, 'values': initial})
    sequence.append({'op': 'solve'})
    sequence.append({'op': 'solve'})
  return {'kind': kind, 'commands': sequence}


def bits(value: float) -> int:
  return struct.unpack('<Q', struct.pack('<d', float(value)))[0]


def trace(request, baseline: Path):
  kind = request['kind']
  prefix, horizon = ('lat', 32) if kind == 'lateral' else ('long', 12)
  name = f'{kind}.acados_ocp_solver_pyx'
  path = baseline / kind / 'c_generated_code/acados_ocp_solver_pyx.so'
  spec = importlib.util.spec_from_file_location(name, path)
  if spec is None or spec.loader is None:
    raise ImportError(str(path))
  module = importlib.util.module_from_spec(spec)
  sys.modules[name] = module
  spec.loader.exec_module(module)
  solver = module.AcadosOcpSolverCython(prefix, 'SQP_RTI', horizon)
  output = []
  fields = {
    'state': 'x',
    'control': 'u',
    'parameters': 'p',
    'weights': 'W',
    'reference': 'yref',
    'lower_slack': 'Zl',
    'lower_bound': 'lbx',
    'upper_bound': 'ubx',
  }
  for command in request['commands']:
    match command['op']:
      case 'reset':
        solver.reset()
      case 'set':
        stage, field = command['stage'], command['field']
        values = np.array(command['values'], dtype=float)
        match field:
          case 'weights':
            size = int(np.sqrt(len(values)))
            solver.cost_set(stage, fields[field], values.reshape((size, size), order='F'))
          case 'reference' | 'lower_slack':
            solver.cost_set(stage, fields[field], values)
          case 'lower_bound' | 'upper_bound':
            solver.constraints_set(stage, fields[field], values)
          case 'state' | 'parameters' | 'control':
            solver.set(stage, fields[field], values)
          case _:
            raise ValueError(field)
      case 'solve':
        status = solver.solve()
        output.append(
          {
            'status': status,
            'x': [[bits(v) for v in solver.get(i, 'x')] for i in range(horizon + 1)],
            'u': [[bits(v) for v in solver.get(i, 'u')] for i in range(horizon)],
            'cost': bits(solver.get_cost()),
          }
        )
      case _:
        raise ValueError(command['op'])
  return output


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--baseline', required=True, type=Path)
  parser.add_argument('--evidence', required=True, type=Path)
  parser.add_argument('--trace', type=Path)
  parser.add_argument('--artifact', type=Path)
  args = parser.parse_args()
  requests = [commands(kind) for kind in ('lateral', 'longitudinal')]
  expected = [trace(request, args.baseline.resolve()) for request in requests]
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / 'input.json').write_text(json.dumps(requests) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected) + '\n')
  if args.trace is not None:
    if args.artifact is None:
      raise ValueError('--trace requires --artifact')
    output = args.evidence / 'native.json'
    child = subprocess.run(
      [str(args.trace.resolve()), str(output.resolve())],
      input=json.dumps(requests),
      text=True,
      capture_output=True,
      check=False,
      env=dict(os.environ, PLANNER_ACADOS=str(args.artifact.resolve())),
    )
    (args.evidence / 'native.stdout').write_text(child.stdout)
    (args.evidence / 'native.stderr').write_text(child.stderr)
    child.check_returncode()
    if json.loads(output.read_text()) != expected:
      raise AssertionError('acados native/Cython outputs differ at the f64 bit level')
    print(json.dumps({'bit_exact_solves': sum(map(len, expected))}))


if __name__ == '__main__':
  main()
