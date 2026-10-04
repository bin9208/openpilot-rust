from __future__ import annotations

import contextlib
import io
from pathlib import Path
from typing import Final

from pytest import MonkeyPatch

PARSERS: Final = ('rcp_tracks', 'rcp_scc', 'rcp_corner_objects', 'rcp_corner_objects_180', 'rcp_corner_objects_430')


def backend_snapshot(state):
  from radarcan_decoder_source import parser_snapshot
  names = ('canfd', 'radar_group1', 'radar_group3', 'radar_group4', 'radar_start_addr', 'radar_msg_count',
    'radar_required_msg_count', 'radar_tracks', 'corner_object_tracks', 'corner_object_180_tracks',
    'corner_object_430_tracks', 'corner_object_missed_updates', 'corner_object_180_missed_updates',
    'corner_object_430_missed_updates', 'trigger_msg_scc', 'trigger_msg_tracks', 'trigger_msg_corner_objects',
    'trigger_msg_corner_objects_180', 'trigger_msg_corner_objects_430', 'corner_objects_available',
    'radar_off_can', 'vRel_last', 'dRel_last')
  output = {name: getattr(state, name) for name in names}
  for name in ('updated_tracks', 'updated_scc', 'updated_corner_objects', 'updated_corner_objects_180', 'updated_corner_objects_430'):
    output[name] = list(getattr(state, name))
  for name in ('corner_object_430_prev_d_rel', 'corner_object_430_prev_v_rel', 'corner_object_430_prev_y_rel',
               'corner_object_430_prev_yv_rel', 'corner_object_430_prev_code', 'corner_object_430_history',
               'corner_object_430_noncenter_inward_frames'):
    output[name] = getattr(state, name)
  manager = state.corner_object_track_ids
  output['corner_object_track_ids'] = {'next_track_id': manager.next_track_id, 'source_cycles': manager.source_cycles.copy(),
    'track_states': [[list(key), list(value)] for key, value in manager.track_states.items()]}
  group3 = state.group3_track_ids
  output['group3_track_ids'] = {'next_id': group3.next_id,
    'previous': {str(key): [value[0], vars(value[1]).copy()] for key, value in group3.previous.items()}}
  output['parsers'] = {role: parser_snapshot(getattr(state, role)) for role in PARSERS}
  return output


def cases(directory: Path):
  from opendbc.can.packer import CANPacker
  from radarcan_decoder_source import metadata
  profiles = [
    ('legacy-scc', 'HYUNDAI_SONATA', 0, 0, 0),
    ('legacy-32-64-slots', 'HYUNDAI_SONATA', 0, 0, 1),
    ('legacy-camera-scc', 'HYUNDAI_SONATA', 8, 0, 1),
    ('legacy-group4', 'HYUNDAI_SONATA', 0, 1 << 15, 1),
    ('canfd-group1', 'HYUNDAI_IONIQ_5', 8193, 1 << 7, 1),
    ('canfd-group2', 'HYUNDAI_IONIQ_5', 8193, 0, 1),
    ('canfd-group3', 'HYUNDAI_IONIQ_5', 8193, 1 << 11, 1),
    ('canfd-corners235-180', 'HYUNDAI_IONIQ_5', 8193, (1 << 12) | (1 << 13), 1),
    ('canfd-corners235-180-expiry', 'HYUNDAI_IONIQ_5', 8193, (1 << 12) | (1 << 13), 1),
    ('canfd-corner-only', 'HYUNDAI_IONIQ_5', 8193, (1 << 12) | (1 << 13) | (1 << 14), 0),
  ]
  output = []
  for name, candidate, flags, ext_flags, tracks in profiles:
    case = {'name': 'decoder-hyundai-' + name, 'op': 'decoder', 'candidate': candidate, 'brand': 'hyundai',
      'delay': 0.0, 'period': .05, 'unavailable': name == 'canfd-corner-only', 'constructor_ns': 1_000_000_000,
      'flags': flags, 'ext_flags': ext_flags, 'dbc_root': str(directory.resolve()),
      'params': {'EnableRadarTracks': str(tracks), 'EnableCornerRadar': '1', 'HyundaiCameraSCC': '0'}, 'actions': []}
    with contextlib.redirect_stdout(io.StringIO()):
      case['parsers'] = metadata(case)
      with MonkeyPatch.context() as fixture:
        fixture.setattr('opendbc.can.dbc.DBC_PATH', str(directory.resolve()))
        packers = {role: CANPacker(parser['dbc']) for role, parser in case['parsers'].items() if parser is not None}
    limit = 170 if name.endswith('-expiry') else 120
    for tick in range(limit):
      frames = []
      for role, packer in packers.items():
        cadence = 5 if role == 'rcp_tracks' else 2 if role == 'rcp_scc' else 3
        pause_end = 140 if name.endswith('-expiry') else 115
        if tick % cadence or (role.startswith('rcp_corner') and 70 <= tick < pause_end):
          continue
        parser = case['parsers'][role]
        for address, _frequency in parser['messages']:
          if name == 'legacy-32-64-slots' and address >= 0x520 and tick >= 50:
            continue
          message = packer.dbc.addr_to_msg[address].name
          values = signals(case, role, address, tick)
          encoded = packer.make_can_msg(message, parser['bus'], values)
          frames.append({'address': encoded[0], 'data': list(encoded[1]), 'bus': encoded[2]})
      case['actions'].append({'v_ego': 12. + tick * .01, 'a_ego': -.2, 'time': (1_000_000_000 + tick * 10_000_000) * 1e-9,
        'packets': [{'mono_time': 1_000_000_000 + tick * 10_000_000, 'frames': frames}]})
    output.append(case)
  return output


def signals(case, role, address, tick):
  active = tick < 40 or tick >= 50
  distance = 20. - tick * .01
  match role:
    case 'rcp_tracks':
      if case['flags'] & 8192:
        if case['ext_flags'] & (1 << 7):
          return {f'{field}{index}': value for index in (1,2) for field, value in
            [('VALID_CNT', 11 if address == 0x210 and active else 0), ('LONG_DIST', distance + index),
             ('LAT_DIST', 1.2 * index), ('REL_SPEED', -.5), ('REL_ACCEL', -.2), ('LAT_SPEED', .1)]}
        if case['ext_flags'] & (1 << 11):
          slot = 0x400 if tick < 30 else 0x405
          return {'OBJECT_ID': 7 if address == slot and active else 0, 'LONG_DIST': distance,
                  'LAT_DIST': 1.2, 'REL_SPEED': -.5, 'OBJECT_LENGTH': 4.}
        return {'VALID_CNT': 11 if address == 0x3a5 and active else 0, 'VALID': 3, 'LONG_DIST': distance,
                'LAT_DIST': 1.2, 'REL_SPEED': -.5, 'REL_ACCEL': -.2, 'LAT_SPEED': .1}
      if case['ext_flags'] & (1 << 15):
        return {'OBJECT_STATE': 3 if address == 0x500 and active else 0, 'LONG_DIST': distance,
                'LAT_DIST': 1.2, 'REL_SPEED': -.5}
      return {'STATE': 3 if address in (0x500,0x520) and active else 0, 'LONG_DIST': distance,
              'AZIMUTH': 5.0, 'REL_SPEED': -.5, 'REL_ACCEL': -.2}
    case 'rcp_scc':
      if case['flags'] & 8192:
        return {'ACC_ObjDist': distance if active else 0, 'ACC_ObjRelSpd': -.5}
      return {'ACC_ObjStatus': int(active), 'ACC_ObjDist': distance, 'ACC_ObjRelSpd': -.5, 'ACC_ObjLatPos': 1.2}
    case 'rcp_corner_objects':
      slot = 0x235 if tick < 30 else 0x237
      return {'OBJ_QUAL_LEVEL': 3 if address == slot and active else 0, 'OBJ_OBJECT_ID': 5, 'OBJ_AGE': tick,
              'OBJ_REL_POS_X': distance, 'OBJ_REL_POS_Y': 2.0, 'OBJ_REL_VEL_X': -.5,
              'OBJ_REL_VEL_Y': -.2, 'OBJ_REL_ACCEL_X': -.1}
    case 'rcp_corner_objects_180':
      return {f'SLOT{index}_{field}': value for index in (1,2) for field, value in
        [('QUAL_LEVEL', 3 if address == 0x180 and index == 1 and active else 0), ('OBJECT_ID', 8), ('AGE', tick),
         ('REL_POS_X', 0.), ('REL_POS_Y', -2.), ('REL_VEL_X', -.5), ('REL_VEL_Y', .2), ('REL_ACCEL_X', -.1)]}
    case _:
      raise AssertionError(role)
