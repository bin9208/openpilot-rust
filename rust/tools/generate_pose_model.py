#!/usr/bin/env python3
import argparse
from pathlib import Path
import runpy
import sys

import sympy as sp
from sympy.printing.rust import RustCodePrinter
from rednose.helpers import ekf_sym


class Printer(RustCodePrinter):
  def _print_Integer(self, expr):
    return f'{int(expr)}.0'

  def _print_MatrixElement(self, expr):
    return f'{expr.parent}[{int(expr.i) * int(expr.parent.cols) + int(expr.j)}]'

  def _print_Pow(self, expr):
    base, power = expr.as_base_exp()
    value = '(' + self._print(base) + ')'
    if power == sp.Rational(1, 2):
      return value + '.sqrt()'
    if power == -1:
      return value + '.recip()'
    if power.is_Integer:
      return value + f'.powi({int(power)})'
    return value + '.powf(' + self._print(power) + ')'


def symbols(root):
  captured = []
  original, arguments = ekf_sym.gen_code, sys.argv
  try:
    ekf_sym.gen_code = lambda *args: captured.append(args)
    sys.argv = ['pose_kf.py', 'pose', 'unused']
    namespace = runpy.run_path(str(root / 'openpilot/selfdrive/locationd/models/pose_kf.py'), run_name='__main__')
  finally:
    ekf_sym.gen_code, sys.argv = original, arguments
  _, _, transition, dt, state, observations, _, _ = captured[0]
  functions = [('transition', transition, True), ('transition_jacobian', transition.jacobian(state), True)]
  for measurement, kind, _ in observations:
    functions.extend([(f'observe_{kind}', measurement, False), (f'jacobian_{kind}', measurement.jacobian(state), False)])
  return namespace['PoseKalman'], functions


def render(name, expression, time):
  printer = Printer()
  argument = ', dt: f64' if time else ''
  rows = [f'pub fn {name}(state: &[f64; 18]{argument}) -> [f64; {len(expression)}] {{', f'    let mut out = [0.0; {len(expression)}];']
  for i, expr in enumerate(expression):
    if expr != 0:
      rows.append(f'    out[{i}] = {printer.doprint(expr)};')
  if not expression.free_symbols:
    rows.insert(1, '    let _ = state;')
  rows.extend(['    out', '}', ''])
  return '\n'.join(rows)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, default=Path('rust/crates/locationd/src/model'))
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  pose, functions = symbols(root)
  args.output.mkdir(parents=True, exist_ok=True)
  for name, expression, time in functions:
    text = '// Generated from unchanged PoseKalman symbolic equations by generate_pose_model.py.\n'
    (args.output / f'{name}.rs').write_text(text + render(name, expression, time))
  constants = [
    f'pub const INITIAL_X: [f64; 18] = {pose.initial_x.tolist()!r};',
    f'pub const INITIAL_P: [f64; 18] = {pose.initial_P.diagonal().tolist()!r};',
    f'pub const PROCESS_NOISE: [f64; 18] = {pose.Q.diagonal().tolist()!r};',
  ]
  for kind, covariance in pose.obs_noise.items():
    constants.append(f'pub const NOISE_{kind}: [f64; 3] = {covariance.diagonal().tolist()!r};')
  modules = '\n'.join(f'mod {name};\npub use {name}::{name};' for name, _, _ in functions)
  (args.output / 'mod.rs').write_text(modules + '\n' + '\n'.join(constants) + '\n')


if __name__ == '__main__':
  main()
