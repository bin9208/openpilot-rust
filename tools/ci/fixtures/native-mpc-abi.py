#!/usr/bin/env python3
"""Direct C ABI versus actual Rust solver trace, using only Python stdlib.

CI engineering evidence for a reused, authenticated generated provider. This
does not regenerate a solver or replace the original Python/Cython regression.
"""
import argparse
import ctypes as c
import json
import math
import os
from pathlib import Path
import platform
import struct
import subprocess


def requests():
  result = []
  # Same finite matrix/state cases as plannerd_acados_source.py, constructed
  # independently without importing the source generator, NumPy or the Rust API.
  for kind, horizon in (('lateral', 32), ('longitudinal', 12)):
    lateral = kind == 'lateral'
    commands = []
    for speed in (5.0, 20.0, 35.0):
      if speed != 20.0:
        commands.append(dict(op='reset'))
      initial = [0.0, 0.0, 0.001, 0.0] if lateral else [0.0, speed, 0.0]
      for stage in range(horizon+1):
        count = (3 if stage == horizon else 5) if lateral else (5 if stage == horizon else 6)
        diagonal = ([2.0, 0.11, 0.0, 0.04, 700.0] if lateral else [5.0, 0.0, 0.0, 0.0, 200.0, 5.0])[:count]
        weights = [diagonal[row] if row == col else 0.005 if lateral and {row, col} == {0, 1} else 0.0
                   for col in range(count) for row in range(count)]
        values = dict(state=initial, parameters=[speed, 1.5] if lateral else [-4.0, 2.0, 100.0, 0.0, 1.45, 0.8, 2.4, 6.0],
                      weights=weights, reference=[0.01*stage, 0.002*stage, 0.0, 0.0, 0.0][:count] if lateral else [0.0]*count)
        if not lateral and stage < horizon:
          values['lower_slack'] = [1e6, 1e6, 1e6, 100.0]
        commands.extend(dict(op='set', stage=stage, field=field, values=value) for field, value in values.items())
      commands.extend(dict(op='set', stage=0, field=field, values=initial) for field in ('lower_bound', 'upper_bound'))
      commands.extend([dict(op='solve'), dict(op='solve')])
    result.append(dict(kind=kind, commands=commands))
  return result


def bits(value):
  assert math.isfinite(value), 'non-finite solver output'
  return struct.unpack('<Q', struct.pack('<d', value))[0]


def trace(provider, request):
  lateral = request['kind'] == 'lateral'
  prefix, horizon, states = ('lat', 32, 4) if lateral else ('long', 12, 3)
  library = c.CDLL(str(provider/f'libacados_ocp_solver_{prefix}.so'))
  pointer = c.c_void_p

  def function(name, result, arguments):
    value = getattr(library, name)
    value.restype, value.argtypes = result, arguments
    return value

  def capsule_function(name, result, arguments):
    return function(f'{prefix}_acados_{name}', result, arguments)

  allocate = capsule_function('create_capsule', pointer, [])
  create = capsule_function('create', c.c_int, [pointer])
  free = capsule_function('free', c.c_int, [pointer])
  release = capsule_function('free_capsule', c.c_int, [pointer])
  solve = capsule_function('solve', c.c_int, [pointer])
  reset = capsule_function('reset', c.c_int, [pointer, c.c_int])
  parameters = capsule_function('update_params', c.c_int, [pointer, c.c_int, c.POINTER(c.c_double), c.c_int])
  capsule = allocate()
  assert capsule
  initialized = False
  try:
    assert create(capsule) == 0
    initialized = True
    config, dims, inputs, outputs, solver = [capsule_function(f'get_nlp_{name}', pointer, [pointer])(capsule)
                                          for name in ('config', 'dims', 'in', 'out', 'solver')]
    assert all((config, dims, inputs, outputs, solver))
    args = [pointer, pointer, pointer, c.c_int, c.c_char_p, pointer]
    cost_set = function('ocp_nlp_cost_model_set', c.c_int, args)
    constraints_set = function('ocp_nlp_constraints_model_set', c.c_int, args)
    output_set = function('ocp_nlp_out_set', None, args)
    output_get = function('ocp_nlp_out_get', None, args)
    evaluate_cost = function('ocp_nlp_eval_cost', None, [pointer, pointer, pointer])
    get = function('ocp_nlp_get', None, [pointer, pointer, c.c_char_p, pointer])
    names = dict(state=b'x', control=b'u', weights=b'W', reference=b'yref', lower_slack=b'Zl', lower_bound=b'lbx', upper_bound=b'ubx')
    result = []
    for command in request['commands']:
      if command['op'] == 'reset':
        assert reset(capsule, 1) == 0
      elif command['op'] == 'set':
        field, stage, values = command['field'], command['stage'], command['values']
        buffer = (c.c_double*len(values))(*values)
        if field == 'parameters':
          assert parameters(capsule, stage, buffer, len(values)) == 0
        elif field in ('state', 'control'):
          output_set(config, dims, outputs, stage, names[field], buffer)
        elif field in ('weights', 'reference', 'lower_slack'):
          assert cost_set(config, dims, inputs, stage, names[field], buffer) == 0
        else:
          assert field in ('lower_bound', 'upper_bound') and stage == 0
          assert constraints_set(config, dims, inputs, stage, names[field], buffer) == 0
      else:
        assert command['op'] == 'solve'
        status = solve(capsule)
        assert status == 0, f'{request["kind"]} solve failed: {status}'
        def output(stage, field, count):
          buffer = (c.c_double*count)()
          output_get(config, dims, outputs, stage, names[field], buffer)
          return list(map(bits, buffer))
        x = [output(stage, 'state', states) for stage in range(horizon+1)]
        u = [output(stage, 'control', 1) for stage in range(horizon)]
        evaluate_cost(solver, inputs, outputs)
        cost = c.c_double()
        get(config, solver, b'cost_value', c.byref(cost))
        result.append(dict(status=status, x=x, u=u, cost=bits(cost.value)))
    return result, library
  finally:
    if initialized:
      assert free(capsule) == 0
    assert release(capsule) == 0


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--provider', type=Path, required=True)
  parser.add_argument('--rust-binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  assert platform.machine() == 'aarch64'
  args.output.mkdir()
  inputs = requests()
  rows = [trace(args.provider, request) for request in inputs]
  expected = [row[0] for row in rows]  # Keep the library handles alive for /proc maps.
  (args.output/'input.json').write_text(json.dumps(inputs)+'\n')
  (args.output/'reference.json').write_text(json.dumps(expected)+'\n')
  (args.output/'provider-maps.txt').write_text(Path('/proc/self/maps').read_text())
  actual = args.output/'rust.json'
  argv = [str(args.rust_binary), str(actual)]
  with (args.output/'rust.stdout').open('w') as stdout, (args.output/'rust.stderr').open('w') as stderr:
    subprocess.run(argv, input=json.dumps(inputs), text=True, stdout=stdout, stderr=stderr,
                   env=dict(os.environ, PLANNER_ACADOS=str(args.provider)), check=True)
  assert json.loads(actual.read_text()) == expected, 'actual Rust/direct C ABI outputs differ at the f64 bit level'
  assert list(map(len, expected)) == [6, 6]
  print(json.dumps(dict(bit_exact_solves=12, lateral_solves=6, longitudinal_solves=6,
                       successful_statuses=True, finite_outputs=True, rust_argv=argv,
                       python_runtime_used_for_reference=True, original_cython_regression=False,
                       solvers_regenerated=False)))


if __name__ == '__main__':
  main()
