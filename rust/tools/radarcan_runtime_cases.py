from __future__ import annotations

import copy
import contextlib
import io
from pathlib import Path

from radarcan_runtime_types import Case


def publication_count(case: Case) -> int:
  from pytest import MonkeyPatch
  from radarcan_decoder_source import constructor
  from radarcan_settings_source import settings
  with settings(case.get('params')), MonkeyPatch.context() as fixture, contextlib.redirect_stdout(io.StringIO()):
    fixture.setattr('opendbc.can.dbc.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.car.hyundai.radar_interface.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.can.parser.time.monotonic_ns', lambda: case['constructor_ns'])
    state = constructor(case)
    count = 0
    for action in case['actions']:
      packets = [(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus'])
        for frame in packet['frames']]) for packet in action['packets']]
      count += state.update_carrot(action['v_ego'], action['a_ego'], action['time'], packets) is not None
  return count


def normal_cases(directory: Path) -> list[Case]:
  from radarcan_decoder_cases import cases as decoder_cases
  from radarcan_hyundai_source import cases as hyundai_cases
  values = [case for case in decoder_cases(directory) if not case['name'].endswith('-unavailable')]
  values.extend(hyundai_cases(directory))
  for case in values:
    case['expected_publications'] = publication_count(case)
    case['name'] = case['name'].replace('decoder-', 'ipc-', 1)
  meb = next(case for case in values if case['candidate'] == 'VOLKSWAGEN_ID4_MK1')
  for candidate, flip, order in [('VOLKSWAGEN_ID4_MK2', False, 'can-state'), ('VOLKSWAGEN_ID4_MK1', True, 'state-can')]:
    case = copy.deepcopy(meb)
    case.update(candidate=candidate, flip=flip, order=order, name=f'ipc-{candidate}-flip{flip}-{order}')
    values.append(case)
  gm = copy.deepcopy(next(case for case in values if case['candidate'] == 'CHEVROLET_VOLT'))
  gm.update(name='ipc-GM-state-before-can', order='state-can')
  values.append(gm)
  fallback = {'name': 'ipc-MOCK-fallback', 'candidate': 'MOCK', 'brand': 'mock', 'delay': 0., 'period': .05,
    'unavailable': True, 'constructor_ns': 1_000_000_000, 'dbc_root': str(directory), 'actions': copy.deepcopy(gm['actions'])}
  fallback['expected_publications'] = publication_count(fallback)
  values.append(fallback)
  return values


def joined_cases(reference: Case) -> list[Case]:
  base = copy.deepcopy(reference)
  template = base['actions'][0]['packets'][0]['frames']
  base['actions'] = [{'time': (1_000_000_000 + tick * 40_000_000) * 1e-9, 'v_ego': 12. + tick * .01, 'a_ego': -.2,
    'packets': [{'mono_time': 1_000_000_000 + tick * 40_000_000, 'frames': copy.deepcopy(template)}]} for tick in range(11)]
  base.update(expected_publications=10, expected_error_publications=0)
  split = copy.deepcopy(base)
  split.update(name='ipc-joined-two-packet-state-first', order='state-can')
  for action in split['actions']:
    packet = action['packets'][0]
    frames = packet['frames']
    first_count = max(1, len(frames)//2)
    action['packets'] = [{**packet, 'frames': frames[:first_count]},
      {**packet, 'mono_time': packet['mono_time'] + 1, 'frames': frames[first_count:]}]
    action['can_delay_ms'] = 8.
  missing = copy.deepcopy(base)
  missing.update(name='ipc-joined-missing-packet-reset-flip-latch', expected_publications=9,
    expected_error_publications=1, flip=True, params_after_first={'RadarTrackFlip': '0'})
  action = missing['actions'][2]
  timestamp = action['packets'][0]['mono_time']
  action['metadata'] = {'firstCanMonoTime': timestamp, 'lastCanMonoTime': timestamp + 1,
    'canPacketCount': 2, 'receiveMonoTime': timestamp}
  stale = copy.deepcopy(base)
  stale.update(name='ipc-joined-stale-state-reset', expected_publications=9, expected_error_publications=1)
  action = stale['actions'][2]
  timestamp = action['packets'][0]['mono_time']
  action['metadata'] = {'firstCanMonoTime': timestamp, 'lastCanMonoTime': timestamp,
    'canPacketCount': 1, 'receiveMonoTime': timestamp - 200_000_000}
  overflow = copy.deepcopy(base)
  overflow.update(name='ipc-joined-33-state-overflow', expected_publications=10, expected_error_publications=1, prequeue=True)
  overflow['actions'][0]['packets'] = []
  overflow['actions'][0]['state_count'] = 33
  return [split, missing, stale, overflow]
