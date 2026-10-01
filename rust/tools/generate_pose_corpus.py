#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

import numpy as np
from rednose.helpers import load_code


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--oracle', type=Path, required=True)
  parser.add_argument('--output', type=Path, default=Path('rust/crates/locationd/tests/data/model.json'))
  args = parser.parse_args()
  ffi, library = load_code(str(args.oracle), 'pose')
  random = np.random.default_rng(138)
  rows = []
  for i in range(16):
    state = random.normal(0.0, 1.0, 18)
    state[:3] *= 0.3
    state[3:6] *= 20.0
    state[9:12] *= 0.1
    state[12:15] *= 10.0
    dt = (0.0, 0.001, 0.05, 0.8)[i % 4]
    if i == 0:
      state[:] = 0.0
    input_state = ffi.new('double[]', state.tolist())
    unused = ffi.new('double[]', [0.0])
    functions = {}
    for name, size, argument in [('f_fun', 18, dt), ('F_fun', 324, dt)] + [
      (f'{prefix}_{kind}', size, unused) for kind in (4, 10, 13, 14) for prefix, size in [('h', 3), ('H', 54)]
    ]:
      output = ffi.new('double[]', size)
      getattr(library, 'pose_' + name)(input_state, argument, output)
      functions[name] = list(output)
    rows.append({'state': state.tolist(), 'dt': dt, 'functions': functions})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(rows, indent=2) + '\n')
  print('Generated 16 actual-source model/Jacobian samples:', args.output)


if __name__ == '__main__':
  main()
