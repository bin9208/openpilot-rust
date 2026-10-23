#!/usr/bin/env python3
"""Hosted Model X admission controls: actual dataclass, full parameters and real constructors."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import itertools
import json
import math
from pathlib import Path
import subprocess
import sys
import traceback
import types

from can_source import ROOT, load
from card_qa.tesla.source import Settings

BASELINE = 'b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a'


def finite(value):
  if isinstance(value, float):
    assert math.isfinite(value)
  elif isinstance(value, dict):
    for item in value.values():
      finite(item)
  elif isinstance(value, list):
    for item in value:
      finite(item)


def baseline_control(output: Path) -> dict:
  from opendbc.car.tesla.interface import CarInterface
  paths = ['opendbc_repo/opendbc/car/interfaces.py'] + [
    f'opendbc_repo/opendbc/car/torque_data/{name}.toml' for name in ('params', 'override', 'substitute')]
  source = {path: subprocess.check_output(['git', 'show', f'{BASELINE}:{path}'], cwd=ROOT) for path in paths}
  directory = output / 'baseline-source'
  directory.mkdir()
  for path, data in source.items():
    (directory / Path(path).name).write_bytes(data)
  module = types.ModuleType('_model_x_497_baseline_interfaces')
  sys.modules[module.__name__] = module
  settings = Settings({})
  try:
    exec(compile(source[paths[0]], f'{BASELINE}:{paths[0]}', 'exec'), module.__dict__)
    # Only the data-file and Params boundaries change; all baseline function bodies stay original.
    module.TORQUE_PARAMS_PATH = directory / 'params.toml'
    module.TORQUE_OVERRIDE_PATH = directory / 'override.toml'
    module.TORQUE_SUBSTITUTE_PATH = directory / 'substitute.toml'
    module.Params = lambda: settings
    try:
      module.CarInterfaceBase.get_params.__func__(CarInterface, 'TESLA_MODEL_X',
        {bus: {} for bus in range(8)}, [], False, True, False)
    except KeyError as error:
      assert error.args == ('TESLA_MODEL_X',)
      (output / 'baseline-failure.log').write_text(traceback.format_exc())
    else:
      raise AssertionError('immutable baseline unexpectedly admitted Model X')
    assert not settings.writes
  finally:
    sys.modules.pop(module.__name__)
  return {'commit': BASELINE, 'failure': 'KeyError: TESLA_MODEL_X', 'writes': settings.writes,
          'sha256': {path: hashlib.sha256(data).hexdigest() for path, data in source.items()}}


def source_controls() -> dict:
  from opendbc.car import Bus, interfaces, structs
  from opendbc.car.tesla.interface import CarInterface
  from opendbc.car.tesla.radar_interface import RadarInterface
  from opendbc.car.tesla.values import CAR, DBC, TeslaFlags
  from opendbc.can import parser
  from opendbc.car.values import PLATFORMS

  # This executes the real default_factory; the generic pt/main catalog field cannot describe party.
  assert CAR.TESLA_MODEL_X.config.dbc_dict == {Bus.party: 'tesla_model3_party'}
  assert DBC[CAR.TESLA_MODEL_X] == {Bus.party: 'tesla_model3_party'}
  catalog = json.loads((ROOT / 'rust/crates/card/data/vehicle.json').read_text())
  tesla = {row['candidate']: row for row in catalog['platforms'] if row['brand'] == 'tesla'}
  for candidate in ('TESLA_MODEL_3', 'TESLA_MODEL_Y', 'TESLA_MODEL_X'):
    assert tesla[candidate]['dbc_pt'] is None and PLATFORMS[candidate].config.dbc_dict[Bus.party] == 'tesla_model3_party'
  raw = interfaces.get_torque_params()['TESLA_MODEL_X']
  assert math.isnan(raw['LAT_ACCEL_FACTOR']) and math.isnan(raw['FRICTION'])
  assert raw['MAX_LAT_ACCEL_MEASURED'] == 2.5
  rows = []
  for nnff, alpha, vehicle in itertools.product((False, True), repeat=3):
    settings = Settings({'NNFF': str(int(nnff))})
    interfaces.Params = lambda: settings
    fingerprint = {bus: {} for bus in range(8)}
    fingerprint[1][0x410] = 8  # A radar frame cannot add a missing radar DBC mapping.
    if vehicle:
      fingerprint[1][0x3df] = 8
    cp = CarInterface.get_params('TESLA_MODEL_X', fingerprint, [], alpha, True, False)
    assert cp.dashcamOnly and cp.radarUnavailable
    assert cp.steerControlType == structs.CarParams.SteerControlType.angle
    assert cp.lateralTuning.which() == 'pid' and cp.maxLateralAccel == 2.5
    finite(cp.to_dict())
    assert bool(cp.flags & TeslaFlags.HAS_VEHICLE_BUS) == vehicle
    assert cp.flags & TeslaFlags.MISSING_DAS_SETTINGS
    assert not cp.flags & TeslaFlags.FSD_14
    now = 2_000_000_000
    parser.time = types.SimpleNamespace(monotonic_ns=lambda: now)
    interface = CarInterface(cp)
    assert interface.CC is not None
    assert interface.can_parsers[Bus.party].dbc_name == 'tesla_model3_party'
    assert interface.can_parsers[Bus.ap_party].dbc_name == 'tesla_model3_party'
    assert (Bus.adas in interface.can_parsers) == vehicle
    if vehicle:
      assert interface.can_parsers[Bus.adas].dbc_name == 'tesla_model3_vehicle'
    assert RadarInterface(cp).rcp is None
    for tick in range(33):
      now = 2_000_000_000 + tick * 10_000_000
      state = interface.update([])
      assert state.vEgoRaw == 0.
      finite(state.to_dict())
    assert not any(write[0] == 'NNFFModelName' for write in settings.writes)
    rows.append({'nnff': nnff, 'alpha_long': alpha, 'vehicle_bus': vehicle, 'updates': 33,
                 'dashcam_only': cp.dashcamOnly, 'radar_unavailable': cp.radarUnavailable,
                 'params': cp.to_dict(), 'writes': settings.writes})
  return {'cases': rows, 'dbc_contract': 'party default retained; no Model X radar mapping',
          'full_can_controller_comparison': 'check_card_tesla.py: eight Model X runtime cases, 620 updates each'}


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load()
  report = {}
  captured = io.StringIO()
  try:
    with contextlib.redirect_stdout(captured), contextlib.redirect_stderr(captured):
      report['baseline'] = baseline_control(args.output)
      report['current'] = source_controls()
    paths = ['opendbc_repo/opendbc/car/tesla/values.py', 'opendbc_repo/opendbc/car/__init__.py',
             'opendbc_repo/opendbc/car/interfaces.py', 'opendbc_repo/opendbc/car/torque_data/override.toml',
             'rust/crates/card/data/vehicle.json', 'rust/crates/card/data/vehicle-497-provenance.json']
    report['sha256'] = {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths}
  finally:
    (args.output / 'source.log').write_text(captured.getvalue())
    (args.output / 'partial.json').write_text(json.dumps(report, indent=2) + '\n')
  report['status'] = 'pass'
  (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
  print('PASS: actual Model X default DBC, finite angle/dashcam admission, radar negatives and immutable baseline failure')


if __name__ == '__main__':
  main()
