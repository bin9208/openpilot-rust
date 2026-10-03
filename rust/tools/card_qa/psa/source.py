from __future__ import annotations

import contextlib
import io

from can_source import load
from card_qa.mazda.source import Settings


def trace(case):
  load()
  from opendbc.car import interfaces, structs, Bus
  from opendbc.car.psa.carstate import CarState
  from opendbc.car.psa.interface import CarInterface

  settings = Settings(case['settings'])
  interfaces.Params = lambda: settings
  cp = structs.CarParams.new_message(carFingerprint=case['candidate'])
  output = io.StringIO()
  with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
    try:
      if case['op'] == 'params':
        firmware = [structs.CarParams.CarFw.new_message(ecu=fw['ecu'], fwVersion=bytes(fw['fw_version'])) for fw in case['firmware']]
        CarInterface.get_params(case['candidate'], {bus: dict(rows) for bus, rows in case['fingerprints']}, firmware, case['alpha_long'], True, False)
      elif case['op'] == 'constructor':
        CarInterface(cp)
      elif case['op'] == 'state':
        CarState(cp).update({Bus.main: object(), Bus.adas: object(), Bus.cam: object()})
      else:
        raise AssertionError(case['op'])
    except (KeyError, FileNotFoundError, AttributeError) as error:
      failure = {'kind': type(error).__name__, 'message': str(error)}
    else:
      raise AssertionError('pinned source boundary unexpectedly succeeded')
  return {'failure': failure, 'writes': settings.writes, 'prints': output.getvalue().splitlines()}


def cases():
  import itertools

  result = []
  for alpha, nnff, lite, disable, fw in itertools.product((False, True), repeat=5):
    result.append(
      {
        'name': f'params-{alpha}-{nnff}-{lite}-{disable}-{fw}',
        'op': 'params',
        'candidate': 'PSA_PEUGEOT_208',
        'alpha_long': alpha,
        'settings': {'NNFF': str(int(nnff)), 'NNFFLite': str(int(lite)), 'DisableMinSteerSpeed': str(int(disable))},
        'firmware': [{'ecu': 'eps', 'fw_version': list(b'owned-psa-firmware')}] if fw else [],
        'fingerprints': [(bus, []) for bus in range(8)],
      }
    )
  for op in ('constructor', 'state'):
    result.append(
      {'name': op + '-candidate-only', 'op': op, 'candidate': 'PSA_PEUGEOT_208', 'alpha_long': False, 'settings': {}, 'firmware': [], 'fingerprints': []}
    )
  return result
