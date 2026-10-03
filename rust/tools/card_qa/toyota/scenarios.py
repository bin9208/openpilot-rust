from __future__ import annotations

import itertools
from can_source import load
from card_qa.toyota.frames import runtime_steps
from card_qa.toyota.edges import numeric_cases


def cases():
  load()
  from opendbc.car.toyota.values import CAR, ToyotaFlags as F
  from openpilot.cereal import car

  firmwares = [[], [('eps', b'8965B47060\0\0\0\0\0\0')], [('eps', b'owned-unknown-eps')], [('eps', b'\x02rack')],
               [('eps', b'8965B42181\0\0\0\0\0\0')], [('hybrid', b'owned-hybrid')], [('dsu', b'owned-dsu')],
               [('eps', b'owned-eps'), ('hybrid', b'owned-hybrid')]]
  settings = ({}, {'NNFF': '1', 'NNFFLite': '1', 'DisableMinSteerSpeed': '1'})
  result = []
  for candidate, alpha, fw_index, options in itertools.product(CAR, (False, True), range(len(firmwares)), settings):
    result.append({'name': f'params-{candidate}-{alpha}-{fw_index}-{bool(options)}', 'op': 'params', 'candidate': str(candidate),
                   'alpha_long': alpha, 'fingerprints': [[0, [[0x3f6, 8]]]] if options else [],
                   'firmware': [{'ecu': ecu, 'fw_version': list(value)} for ecu, value in firmwares[fw_index]],
                   'settings': options, 'now': 2_000_000_000, 'steps': []})
  for candidate, advanced in itertools.product(CAR, (False, True)):
    profile = {'name': f'runtime-{candidate}-{advanced}', 'op': 'runtime', 'candidate': str(candidate), 'alpha_long': advanced,
               'fingerprints': [[0, [[0x3f6, 8]]]], 'firmware': [{'ecu': ecu, 'fw_version': list(value)} for ecu, value in firmwares[-1]] if advanced else [],
               'settings': {}, 'now': 2_000_000_000, 'steps': []}
    if candidate.config.flags & F.SECOC and advanced: profile['secoc_key'] = list(range(16))
    profile['steps'] = runtime_steps(profile, advanced)
    result.append(profile)
  profiles = [case for case in result if case['op'] == 'runtime']
  for profile, active in itertools.product(profiles[::2], (False, True)):
    control = car.CarControl.new_message(enabled=True, latActive=active, cruiseControl={'cancel': True}, actuators={'torque': 1.})
    result.append({**profile, 'name': f"before-update-{profile['candidate']}-{active}", 'op': 'before_update',
                   'steps': [{**profile['steps'][0], 'packets': [], 'control': list(control.to_bytes())}]})
  for profile in profiles:
    if profile['candidate'] == 'TOYOTA_RAV4_TSS2' and profile['alpha_long']:
      result.append({**profile, 'name': 'runtime-rav4-nnff-controller-limits', 'settings': {'NNFF': '1'}})
      break
  result.extend(numeric_cases(profiles[0]))
  return result
