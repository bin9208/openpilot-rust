#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

import numpy as np
from rednose.helpers import load_code


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--output', type=Path, default=Path('rust/crates/paramsd/tests/data/model.json'))
  args = parser.parse_args()
  ffi, library = load_code(str(args.oracle), 'car')
  random = np.random.default_rng(143)
  rows = []
  names = ('mass', 'rotational_inertia', 'center_to_front', 'center_to_rear', 'stiffness_front', 'stiffness_rear')
  for i in range(24):
    state = random.normal(0., .1, 9)
    state[0], state[1], state[4] = random.uniform(.2, 5.), random.uniform(7., 30.), random.uniform(1., 40.)
    globals_values = [random.uniform(1200., 3200.), random.uniform(2000., 6500.), random.uniform(.9, 1.5),
                      random.uniform(1.2, 1.8), random.uniform(60000., 170000.), random.uniform(60000., 170000.)]
    for name, value in zip(names, globals_values, strict=True):
      getattr(library, 'car_set_' + name)(value)
    dt = (0., .001, .05, .8)[i % 4]
    input_state, unused = ffi.new('double[]', state.tolist()), ffi.new('double[]', [0.])
    functions = {}
    for name, size, argument in [('f_fun', 9, dt), ('F_fun', 81, dt)] + [
      (f'{prefix}_{kind}', dim * factor, unused) for kind in range(24, 32) for dim in [2 if kind == 24 else 1]
      for prefix, factor in [('h', 1), ('H', 9)]
    ]:
      output = ffi.new('double[]', size)
      getattr(library, 'car_' + name)(input_state, argument, output)
      functions[name] = list(output)
    rows.append({'state': state.tolist(), 'globals': globals_values, 'dt': dt, 'functions': functions})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(rows, indent=2) + '\n')
  print('Generated 24 actual-source car model/Jacobian samples:', args.output)


if __name__ == '__main__':
  main()
