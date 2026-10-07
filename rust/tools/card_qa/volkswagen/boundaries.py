# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy==2.5.3", "pycapnp==2.1.0"]
# ///
# How to run: oracle Python rust/tools/card_qa/volkswagen/boundaries.py
from __future__ import annotations

import contextlib
import io
import json
import tempfile
from pathlib import Path
from can_source import ROOT, load
from card_qa.mazda.source import Settings
from card_qa.volkswagen.source import parameters
from card_qa.volkswagen.scenarios import profile


def main():
  load()
  from opendbc.car.volkswagen.values import CAR
  from opendbc.car.volkswagen.carstate import CarState
  from opendbc.can import CANDefine
  import opendbc.can.dbc as source_dbc
  rows = [(CAR.VOLKSWAGEN_PASSAT_NMS, 1, 'vw_pq', 'Getriebe_1', 'Waehlhebelposition__Getriebe_1_', 'Lenkhilfe_2', 'LH2_Sta_HCA'),
          (CAR.VOLKSWAGEN_GOLF_MK7, 1, 'vw_mqb', 'Gateway_73', 'GE_Fahrstufe', 'LH_EPS_03', 'EPS_HCA_Status'),
          (CAR.VOLKSWAGEN_GOLF_MK7, 2, 'vw_mqb', 'Motor_EV_01', 'MO_Waehlpos', 'LH_EPS_03', 'EPS_HCA_Status'),
          (CAR.VOLKSWAGEN_ID4_MK1, 1, 'vw_meb', 'Getriebe_11', 'GE_Fahrstufe', 'QFK_01', 'LatCon_HCA_Status'),
          (CAR.VOLKSWAGEN_ID4_MK2, 1, 'vw_meb_2024', 'Getriebe_11', 'GE_Fahrstufe', 'QFK_01', 'LatCon_HCA_Status')]
  original_root = source_dbc.DBC_PATH
  results = []
  for candidate, detected, dbc, gear_name, gear_signal, hca_name, hca_signal in rows:
    with contextlib.redirect_stdout(io.StringIO()):
      cp = parameters(profile(candidate, False, False, detected), Settings({}))
    source = (ROOT / f'opendbc_repo/opendbc/dbc/{dbc}.dbc').read_text()
    definitions = CANDefine(dbc).dv
    gear_error = gear_signal if len(definitions[gear_name]) > 1 else gear_name
    hca_error = hca_signal if len(definitions[hca_name]) > 1 else hca_name
    for scenario, omitted, expected in [('missing-dbc', None, None), ('missing-gear-and-hca', (gear_signal, hca_signal), gear_error), ('missing-hca', (hca_signal,), hca_error)]:
      with tempfile.TemporaryDirectory(prefix='vw-boundary-') as directory:
        source_dbc.DBC_PATH = directory
        source_dbc.DBC.cache_clear()
        if omitted is not None:
          text = '\n'.join(line for line in source.splitlines() if not line.startswith('VAL_ ') or line.split()[2] not in omitted) + '\n'
          Path(directory, f'{dbc}.dbc').write_text(text)
        try:
          CarState(cp)
        except (FileNotFoundError, KeyError) as error:
          assert type(error) == (FileNotFoundError if expected is None else KeyError)
          if expected is not None:
            assert error.args == (expected,), (str(candidate), scenario, error.args, expected)
          results.append({'candidate': str(candidate), 'dbc': dbc, 'scenario': scenario, 'kind': type(error).__name__, 'detail': str(error)})
        else:
          raise AssertionError('original constructor unexpectedly succeeded')
        source_dbc.DBC_PATH = original_root
        source_dbc.DBC.cache_clear()
  print(json.dumps({'status': 'pass', 'cases': results}, indent=2))


if __name__ == '__main__':
  main()
