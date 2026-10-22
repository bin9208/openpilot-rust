#!/usr/bin/env python3
"""Cloud-only source failure controls; no injected NumPy or substituted vehicle policy."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import math
from pathlib import Path
import subprocess
import types

from can_source import ROOT, load
from card_qa.mazda.source import Settings

BASELINE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'


def baseline_module(brand: str) -> tuple[types.ModuleType, dict]:
  path = f'opendbc_repo/opendbc/car/{brand}/carstate.py'
  source = subprocess.check_output(['git', 'show', f'{BASELINE}:{path}'], cwd=ROOT)
  module = types.ModuleType(f'_vehicle_497_baseline_{brand}')
  exec(compile(source, f'{BASELINE}:{path}', 'exec'), module.__dict__)
  return module, {'path': path, 'baseline_sha256': hashlib.sha256(source).hexdigest(),
                  'current_sha256': hashlib.sha256((ROOT / path).read_bytes()).hexdigest()}


def mqb_controls() -> dict:
  from opendbc.car.volkswagen.interface import CarInterface
  from opendbc.car.volkswagen.values import CAR, VolkswagenFlags as F
  from card_qa.volkswagen.scenarios import profile
  from card_qa.volkswagen.source import parameters
  old, identity = baseline_module('volkswagen')
  current = CarInterface.CarState
  selected = [car for car in CAR if not car.config.flags & (F.PQ | F.MEB)]
  assert len(selected) == 23
  rows = []
  try:
    for car in selected:
      cp = parameters(profile(car, False, False, 1), Settings({}))
      CarInterface.CarState = old.CarState
      original = CarInterface(cp)
      try:
        original.update([])
      except NameError as failure:
        assert str(failure) == "name 'np' is not defined"
      else:
        raise AssertionError(f'baseline MQB unexpectedly succeeded: {car}')
      CarInterface.CarState = current
      repaired = CarInterface(cp)
      first = repaired.update([])
      assert first.standstill and first.vEgoRaw == 0.
      assert repaired.CS.frame == 1
      # Uses the real constructor/update, rather than a seeded controller or np global.
      for _ in range(32):
        repaired.update([])
      assert repaired.CS.frame == 33
      rows.append({'candidate': str(car), 'baseline': 'NameError: np', 'current_updates': 33})
  finally:
    CarInterface.CarState = current
  return {**identity, 'cases': rows}


def tesla_controls() -> dict:
  from opendbc.car import interfaces
  from opendbc.car.tesla.interface import CarInterface
  from opendbc.can import CANPacker, parser
  old, identity = baseline_module('tesla')
  current = CarInterface.CarState
  rows = []
  now = 2_000_000_000
  parser.time = types.SimpleNamespace(monotonic_ns=lambda: now)
  try:
    for vehicle_bus in (False, True):
      fingerprint = {bus: {} for bus in range(8)}
      if vehicle_bus:
        fingerprint[1][0x3df] = 8
      outputs = {}
      for label, state_class in (('baseline', old.CarState), ('current', current)):
        interfaces.Params = lambda: Settings({})
        cp = CarInterface.get_params('TESLA_MODEL_3', fingerprint, [], False, True, False)
        CarInterface.CarState = state_class
        ci = CarInterface(cp)
        party = CANPacker('tesla_model3_party')
        vehicle = CANPacker('tesla_model3_vehicle')
        events = []
        for index, (acc, touch) in enumerate(((4, 0), (0, 3), (4, 3), (4, 3), (4, 3), (4, 0))):
          now = 2_000_000_000 + index * 10_000_000
          frames = [party.make_can_msg('DAS_control', 2, {'DAS_accState': acc})]
          if vehicle_bus:
            frames.append(vehicle.make_can_msg('UI_status2', 1, {'UI_activeTouchPoints': touch}))
          state = ci.update([(now, frames)])
          events.append([event.to_dict() for event in state.buttonEvents])
        outputs[label] = events
      expected = [[{'type': 'cancel', 'pressed': pressed}] +
                  ([{'type': 'lkas', 'pressed': pressed}] if vehicle_bus else []) for pressed in (True, False)]
      assert [outputs['current'][i] for i in (1, 5)] == expected
      assert all(not outputs['current'][i] for i in (0, 2, 3, 4))
      if vehicle_bus:
        for index, pressed in ((1, True), (5, False)):
          assert outputs['baseline'][index] == [
            {'type': 'unknown', 'pressed': False}, {'type': 'lkas', 'pressed': pressed}]
      else:
        assert outputs['baseline'] == outputs['current']
      rows.append({'vehicle_bus': vehicle_bus, **outputs})
  finally:
    CarInterface.CarState = current
  return {**identity, 'cases': rows}


def ford_controls() -> dict:
  from opendbc.car import interfaces, structs
  from opendbc.car.ford.interface import CarInterface

  def finite(value):
    if isinstance(value, float):
      assert math.isfinite(value)
    elif isinstance(value, dict):
      for item in value.values():
        finite(item)
    elif isinstance(value, list):
      for item in value:
        finite(item)

  rows = []
  for candidate in ('FORD_ESCAPE_MK4_5', 'FORD_EXPEDITION_MK4'):
    raw = interfaces.get_torque_params()[candidate]
    assert math.isnan(raw['LAT_ACCEL_FACTOR']) and math.isnan(raw['FRICTION'])
    assert raw['MAX_LAT_ACCEL_MEASURED'] == 1.5
    for nnff in (False, True):
      settings = Settings({'NNFF': str(int(nnff))})
      interfaces.Params = lambda: settings
      cp = CarInterface.get_params(candidate, {bus: {} for bus in range(8)}, [], False, True, False)
      assert cp.steerControlType == structs.CarParams.SteerControlType.angle
      assert cp.lateralTuning.which() == 'pid' and cp.maxLateralAccel == 1.5
      finite(cp.to_dict())
      assert not any(write[0] == 'NNFFModelName' for write in settings.writes)
      # Actual state/controller construction must work without replacing torque placeholders.
      vehicle = CarInterface(cp)
      assert vehicle.CC is not None
      rows.append({'candidate': candidate, 'nnff': nnff, 'steering': 'angle',
                   'tuning': cp.lateralTuning.which(), 'writes': settings.writes})
  return {'cases': rows, 'placeholders': 'NaN factors retained in table; none admitted to full active CarParams',
          'remaining': ['GMC_YUKON_CC'],
          'source_sha256': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in (
            'opendbc_repo/opendbc/car/torque_data/override.toml', 'opendbc_repo/opendbc/car/ford/interface.py',
            'opendbc_repo/opendbc/car/interfaces.py', 'rust/crates/card/data/vehicle.json',
            'rust/crates/card/data/vehicle-497-provenance.json')}}


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load()
  captured = io.StringIO()
  # Failing controls are actual immutable class definitions loaded from the baseline.
  # Params/clock are the same owned boundaries used by the maintained vehicle oracles.
  report = {'baseline': BASELINE}
  try:
    with contextlib.redirect_stdout(captured), contextlib.redirect_stderr(captured):
      report['mqb'] = mqb_controls()
      report['tesla'] = tesla_controls()
      report['ford'] = ford_controls()
  finally:
    (args.output / 'source.log').write_text(captured.getvalue())
    (args.output / 'partial.json').write_text(json.dumps(report, indent=2) + '\n')
  (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  print('PASS: 23 MQB first-update controls, Tesla cancel/infotainment events, two exact finite Ford angle admissions')


if __name__ == '__main__':
  main()
