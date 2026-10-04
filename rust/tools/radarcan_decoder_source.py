from __future__ import annotations

import copy
from pytest import MonkeyPatch

from radarcan_base_source import data, track_fields
from radarcan_source import normalized, floating_bits
from radarcan_settings_source import settings


def constructor(case):
  from opendbc.car import structs
  from opendbc.car.car_helpers import interfaces
  cp = structs.CarParams(carFingerprint=case['candidate'], radarDelay=case['delay'], radarTimeStep=case['period'],
    radarUnavailable=case['unavailable'], flags=case.get('flags', 0), extFlags=case.get('ext_flags', 0),
    safetyConfigs=[{} for _ in range(case.get('safety_count', 1))])
  return interfaces[cp.carFingerprint].RadarInterface(cp)


def snapshot(state):
  common = {name: getattr(state, name) for name in ('frame', 'v_ego_hist', 'a_ego_hist', 'v_ego', 'a_ego',
    'last_timestamp', 'dt', 'init_samples', 'init_done')}
  common['pts'] = {str(address): point.to_dict() for address, point in state.pts.items()}
  common['pts_order'] = list(state.pts)
  common['tracks'] = {str(track_id): track_fields(track) for track_id, track in state.tracks.items()}
  common['tracks_order'] = list(state.tracks)
  common['history_maxlen'] = state.v_ego_hist.maxlen
  backend = {name: getattr(state, name) for name in ('track_id', 'radar_fault', 'radar_wrong_config', 'valid_cnt')
             if hasattr(state, name)}
  for name in ('_track_id_counter', '_yv_state', 'points', 'scan_index_invalid_cnt', 'radar_unavailable_cnt', 'prev_headerScanIndex'):
    if hasattr(state, name):
      backend[name] = getattr(state, name)
  if hasattr(state, 'clusters'):
    backend['clusters'] = [vars(cluster).copy() for cluster in state.clusters]
  if hasattr(state, 'updated_messages'):
    backend['updated_messages'] = list(state.updated_messages)
  else:
    from radarcan_hyundai_source import backend_snapshot
    backend.update(backend_snapshot(state))
  return normalized({'common': common, 'bits': floating_bits(common), 'backend': backend, 'parser': parser_snapshot(state.rcp)})


def parser_snapshot(parser):
  return None if parser is None else {'bus': parser.bus, 'dbc': parser.dbc_name,
    'states': {str(address): {'values': [parser.vl[address][signal.name] for signal in message.signals],
      'all_values': [parser.vl_all[address][signal.name] for signal in message.signals],
      'timestamps': list(message.timestamps), 'counter': message.counter, 'counter_fail': message.counter_fail,
      'first_seen': message.first_seen_nanos, 'last_warning': message.last_warning_log_nanos,
      'frequency': message.frequency, 'timeout_threshold': message.timeout_threshold}
      for address, message in parser.message_states.items()}, 'invalid_count': parser.can_invalid_cnt,
    'last_nonempty': parser.last_nonempty_nanos, 'last_update': parser._last_update_nanos}


def trace(case):
  from opendbc.can.dbc import DBC
  from opendbc.car.carlog import carlog
  DBC.cache_clear()
  warnings = []
  with settings(case.get('params')) as reads, MonkeyPatch.context() as fixture:
    fixture.setattr('opendbc.can.dbc.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.car.hyundai.radar_interface.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.can.parser.time.monotonic_ns', lambda: case['constructor_ns'])
    fixture.setattr(carlog, 'warning', lambda message: warnings.append(message))
    state = constructor(case)
    output = []
    for action in case['actions']:
      packets = [(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus'])
        for frame in packet['frames']]) for packet in action['packets']]
      result = state.update_carrot(action['v_ego'], action['a_ego'], action['time'], packets)
      output.append(copy.deepcopy({'result': data(result), 'state': snapshot(state), 'warnings': list(warnings)}))
      warnings.clear()
  return normalized({'steps': output, 'parameter_reads': reads}) if case.get('params') is not None else normalized(output)


def metadata(case):
  from opendbc.can.dbc import DBC
  DBC.cache_clear()
  with settings(case.get('params')), MonkeyPatch.context() as fixture:
    fixture.setattr('opendbc.can.dbc.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.car.hyundai.radar_interface.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.can.parser.time.monotonic_ns', lambda: case['constructor_ns'])
    state = constructor(case)
  parser = state.rcp
  if hasattr(state, 'rcp_tracks'):
    from radarcan_hyundai_source import PARSERS
    return {role: None if getattr(state, role) is None else parser_metadata(getattr(state, role)) for role in PARSERS}
  if parser is None:
    return None
  return parser_metadata(parser)


def parser_metadata(parser):
  return {'dbc': parser.dbc_name, 'bus': parser.bus,
          'messages': [[address, message.frequency] for address, message in parser.message_states.items()]}
