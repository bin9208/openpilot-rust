from __future__ import annotations

import itertools
from can_source import load
from card_qa.honda.frames import runtime_steps
from card_qa.honda.edges import error_cases


def cases():
  load()
  from opendbc.car.honda.values import CAR, HondaFlags as F
  result = []
  firmwares = ([], [('eps', b'normal')], [('eps', b'modified,eps')])
  for candidate, alpha, modified, detected, offset, settings in itertools.product(CAR, (False, True), range(3), (False, True), (0, 4), ({}, {'NNFF': '1', 'NNFFLite': '1', 'DisableMinSteerSpeed': '1'})):
    bosch = candidate.config.flags & F.BOSCH and not candidate.config.flags & F.BOSCH_RADARLESS
    pt = offset + int(bool(bosch))
    rows = [[pt, [[0x191, 8], [0x1a3, 8], [0x1be, 8], [0x33da, 8]]], [offset, [[0x12f8bfa7, 8]]]] if detected else [[offset, [[0x999, 8]]]]
    if detected and pt == offset:
      rows = [[pt, [[0x191, 8], [0x1a3, 8], [0x1be, 8], [0x33da, 8], [0x12f8bfa7, 8]]]]
    result.append({'name': f'params-{candidate}-{alpha}-{modified}-{detected}-{offset}-{bool(settings)}', 'op': 'params',
                   'candidate': str(candidate), 'alpha_long': alpha, 'fingerprints': rows,
                   'firmware': [{'ecu': ecu, 'fw_version': list(value)} for ecu, value in firmwares[modified]],
                   'settings': settings, 'now': 2_000_000_000, 'steps': []})
  for candidate, advanced in itertools.product(CAR, (False, True)):
    flags = candidate.config.flags
    pt = int(bool(flags & F.BOSCH and not flags & F.BOSCH_RADARLESS))
    rows = [[pt, [[0x1a3, 8], [0x1be, 8]]], [0, [[0x12f8bfa7, 8]]]] if advanced else []
    if advanced and pt == 0:
      rows = [[0, [[0x1a3, 8], [0x1be, 8], [0x12f8bfa7, 8]]]]
    profile = {'name': f'runtime-{candidate}-{advanced}', 'op': 'runtime', 'candidate': str(candidate),
               'alpha_long': advanced, 'fingerprints': rows, 'firmware': [{'ecu': 'eps', 'fw_version': list(b'modified,eps')}] if advanced else [],
               'settings': {}, 'now': 2_000_000_000, 'steps': []}
    profile['steps'] = runtime_steps(profile, advanced)
    result.append(profile)
  profiles = [case for case in result if case['op'] == 'runtime']
  accord = next(case for case in profiles if case['candidate'] == 'HONDA_ACCORD' and case['alpha_long'])
  cvt = {**accord, 'name': 'runtime-accord-cvt',
         'fingerprints': [[1, [[0x191, 8], [0x1a3, 8], [0x1be, 8]]]]}
  cvt['steps'] = runtime_steps(cvt, True)
  result.append(cvt)
  for candidate in ('HONDA_CIVIC', 'HONDA_ACCORD', 'HONDA_CIVIC_2022'):
    for original in [case for case in profiles if case['candidate'] == candidate]:
      offset = {**original, 'offset': True, 'name': f"offset-{original['name']}"}
      offset['steps'] = runtime_steps(offset, original['alpha_long'])
      result.append(offset)
      ext = {**original, 'flags_or': int(F.BOSCH_EXT_HUD), 'name': f"ext-hud-{original['name']}"}
      result.append(ext)
  for profile in profiles:
    result.append({**profile, 'name': f"before-update-{profile['name']}", 'op': 'before_update', 'steps': [profile['steps'][0]]})
  civic = next(case for case in profiles if case['candidate'] == 'HONDA_CIVIC' and case['alpha_long'])
  pump = {**civic, 'name': 'runtime-nidec-pump-refresh'}
  pump['steps'] = runtime_steps(pump, True, count=2300, pump=True)
  result.append(pump)
  result.extend(error_cases(profiles))
  return result
