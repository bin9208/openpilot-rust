#!/usr/bin/env python3
import argparse
from pathlib import Path
import runpy
import sys

from rednose.helpers import ekf_sym
from generate_pose_model import Printer


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, default=Path('rust/crates/paramsd/src/model'))
  args = parser.parse_args()
  captured = []
  original, arguments = ekf_sym.gen_code, sys.argv
  try:
    ekf_sym.gen_code = lambda *values, **keywords: captured.append((values, keywords))
    sys.argv = ['car_kf.py', 'car', 'unused']
    namespace = runpy.run_path('openpilot/selfdrive/locationd/models/car_kf.py', run_name='__main__')
  finally:
    ekf_sym.gen_code, sys.argv = original, arguments
  values, keywords = captured[0]
  _, _, transition, _, state, observations, _, _ = values
  functions = [('transition', transition, True), ('transition_jacobian', transition.jacobian(state), True)]
  for measurement, kind, _ in observations:
    functions.extend([(f'observe_{kind}', measurement, False), (f'jacobian_{kind}', measurement.jacobian(state), False)])
  args.output.mkdir(parents=True, exist_ok=True)
  printer = Printer()
  for name, expression, time in functions:
    signature = ', globals: &[f64; 6], dt: f64' if time else ''
    lines = ['// Generated from unchanged CarKalman symbolic equations by generate_car_model.py.',
             f'pub fn {name}(state: &[f64; 9]{signature}) -> [f64; {len(expression)}] {{']
    if time:
      lines.extend(f'    let {symbol} = globals[{i}];' for i, symbol in enumerate(keywords['global_vars']))
    if not expression.free_symbols:
      lines.append('    let _ = state;')
    lines.append(f'    let mut out = [0.0; {len(expression)}];')
    for i, expr in enumerate(expression):
      if expr != 0:
        lines.append(f'    out[{i}] = {printer.doprint(expr)};')
    lines.extend(['    out', '}', ''])
    (args.output / f'{name}.rs').write_text('\n'.join(lines))
  model = namespace['CarKalman']
  constants = [f'pub const INITIAL_X: [f64; 9] = {model.initial_x.tolist()!r};',
               f'pub const INITIAL_P: [f64; 9] = {model.P_initial.diagonal().tolist()!r};',
               f'pub const PROCESS_NOISE: [f64; 9] = {model.Q.diagonal().tolist()!r};']
  for kind, covariance in model.obs_noise.items():
    constants.append(f'pub const NOISE_{kind}: f64 = {float(covariance[0, 0])!r};')
  modules = '\n'.join(f'mod {name};\npub use {name}::{name};' for name, _, _ in functions)
  (args.output / 'mod.rs').write_text(modules + '\n' + '\n'.join(constants) + '\n')


if __name__ == '__main__':
  main()
