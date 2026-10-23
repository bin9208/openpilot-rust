#!/usr/bin/env python3
"""Hosted source/public-API control for the inferred exact K5 hybrid key repair."""
from __future__ import annotations

import argparse
import ast
import contextlib
from functools import cache
import hashlib
import io
import itertools
import json
import math
from pathlib import Path
import subprocess
import tempfile
import tomllib

from can_source import ROOT, load
from card_qa.mazda.source import Settings

BASE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'
KEY = 'KIA_K5_DL3_24_HEV'
ROW = 'KIA_K5_DL3_HEV_24'
TORQUE = ROOT / 'opendbc_repo/opendbc/car/torque_data'


def baseline_control():
  from opendbc.car import interfaces
  path = 'opendbc_repo/opendbc/car/interfaces.py'
  original = subprocess.check_output(['git', 'show', f'{BASE}:{path}'], cwd=ROOT)
  function = next(node for node in ast.parse(original).body if isinstance(node, ast.FunctionDef) and node.name == 'get_torque_params')
  maintained = interfaces.get_torque_params
  with tempfile.TemporaryDirectory(prefix='hyundai-key-baseline-') as temporary:
    scope = {'cache': cache, 'tomllib': tomllib}
    for filename, variable in [('params.toml', 'TORQUE_PARAMS_PATH'), ('override.toml', 'TORQUE_OVERRIDE_PATH'), ('substitute.toml', 'TORQUE_SUBSTITUTE_PATH')]:
      data = subprocess.check_output(['git', 'show', f'{BASE}:opendbc_repo/opendbc/car/torque_data/{filename}'], cwd=ROOT)
      target = Path(temporary) / filename
      target.write_bytes(data)
      scope[variable] = target
    exec(compile(ast.Module(body=[function], type_ignores=[]), f'{BASE}:{path}', 'exec'), scope)
    try:
      interfaces.get_torque_params = scope['get_torque_params']
      interfaces.CarInterfaceBase.get_std_params(KEY)
    finally:
      interfaces.get_torque_params = maintained


def finite(value):
  if isinstance(value, float):
    assert math.isfinite(value), value
  elif isinstance(value, dict):
    for item in value.values(): finite(item)
  elif isinstance(value, list):
    for item in value: finite(item)


def check(output, native):
  from opendbc.car import interfaces, structs
  from opendbc.car.hyundai import interface, hyundaicanfd
  from opendbc.car.hyundai.values import CAR
  try:
    baseline_control()
  except KeyError as error:
    assert error.args == (KEY,), error
  else:
    raise AssertionError('baseline unexpectedly admits exact hybrid key')
  before = subprocess.check_output(['git', 'show', f'{BASE}:opendbc_repo/opendbc/car/torque_data/params.toml'], cwd=ROOT)
  current = (TORQUE / 'params.toml').read_bytes()
  assert tomllib.loads(before.decode())[ROW] == tomllib.loads(current.decode())[ROW] == [2.5, 2.5, 0.1]
  sub = tomllib.loads((TORQUE / 'substitute.toml').read_text())
  assert sub[KEY] == ROW
  # No normalized-name fallback: this explicit alias has exactly one semantic identity.
  matches = [str(candidate) for candidate in CAR if sorted(str(candidate).split('_')) == sorted(ROW.split('_'))]
  assert matches == [KEY], matches
  interfaces.get_torque_params.cache_clear()
  resolved = interfaces.get_torque_params()
  assert resolved[KEY] == resolved[ROW] == dict(zip(['LAT_ACCEL_FACTOR', 'MAX_LAT_ACCEL_MEASURED', 'FRICTION'], [2.5, 2.5, 0.1], strict=True))
  rows = []
  original_params = [(module, module.Params) for module in (interfaces, interface, hyundaicanfd)]
  try:
    for offset, hda2, angle, nnff, alpha, firmware in itertools.product((0, 4), (False, True), (False, True), (False, True), (False, True), (False, True)):
      name = f'o{offset}-h{int(hda2)}-a{int(angle)}-n{int(nnff)}-l{int(alpha)}-f{int(firmware)}'
      settings = Settings({'NNFF': str(int(nnff)), 'CanfdHDA2': str(int(hda2))})
      for module, _ in original_params: module.Params = lambda: settings
      fingerprint = {bus: {} for bus in range(8)}
      fingerprint[offset + int(hda2)] = {0x105: 32, 0x1cf: 8, 0x1ba: 8, 0x130: 8}
      fingerprint[offset + 2] = {0x2a4: 32, **({0xcb: 24} if angle else {})}
      fw = [structs.CarParams.CarFw.new_message(ecu='eps', fwVersion=b'owned DL3 HEV EPS 2024')] if firmware else []
      captured = io.StringIO()
      with contextlib.redirect_stdout(captured):
        cp = interface.CarInterface.get_params(KEY, fingerprint, fw, alpha, False, False)
      expected = cp.to_dict()
      finite(expected)
      assert cp.carFingerprint == KEY and cp.maxLateralAccel == 2.5
      assert cp.steerControlType == ('angle' if angle else 'torque')
      if not angle:
        assert cp.lateralTuning.which() == 'torque'
        assert cp.lateralTuning.torque.latAccelFactor == 2.5
        assert cp.lateralTuning.torque.friction == 0.10000000149011612
      assert not settings.writes, settings.writes
      (output / f'{name}.bin').write_bytes(cp.to_bytes())
      (output / f'{name}.stdout').write_text(captured.getvalue())
      if native:
        with structs.CarParams.from_bytes((native / f'{name}.bin').read_bytes()) as actual:
          assert actual.to_dict() == expected, (name, actual.to_dict(), expected)
      rows.append({'case': name, 'full_schema_equal': native is not None, 'writes': settings.writes})
  finally:
    for module, saved in original_params: module.Params = saved
  return {'inference': 'unique reordered exact identity; existing fork tune, not verified calibration',
          'baseline': BASE, 'baseline_failure': f'KeyError: {KEY}', 'cases': rows,
          'native_compared': native is not None, 'vehicle_accepted': False,
          'params_file_sha256': hashlib.sha256(current).hexdigest(),
          'source_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in
                           [TORQUE / 'params.toml', TORQUE / 'substitute.toml', ROOT / 'opendbc_repo/opendbc/car/interfaces.py',
                            ROOT / 'opendbc_repo/opendbc/car/hyundai/values.py', ROOT / 'opendbc_repo/opendbc/car/hyundai/interface.py']}}


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--native', type=Path)
  parser.add_argument('--baseline-red', action='store_true')
  args = parser.parse_args()
  load()
  if args.baseline_red:
    # Intentionally exits nonzero from the public API's actual baseline KeyError.
    baseline_control()
    raise AssertionError('baseline unexpectedly admits exact hybrid key')
  args.output.mkdir(parents=True, exist_ok=False)
  result = check(args.output, args.native)
  (args.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({'source_cases': len(result['cases']), 'native_compared': result['native_compared'], 'vehicle_accepted': False}))


if __name__ == '__main__':
  main()
