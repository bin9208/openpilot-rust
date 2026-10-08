from __future__ import annotations

from pytest import MonkeyPatch

from radarcan_decoder_source import constructor, parser_snapshot, snapshot
from radarcan_settings_source import settings
from radarcan_source import floating_bits, normalized


def trace(case):
  from opendbc.can.dbc import DBC
  DBC.cache_clear()
  with settings(case.get('params')) as reads, MonkeyPatch.context() as fixture:
    fixture.setattr('opendbc.can.dbc.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.car.hyundai.radar_interface.DBC_PATH', case['dbc_root'])
    fixture.setattr('opendbc.can.parser.time.monotonic_ns', lambda: case['constructor_ns'])
    try:
      state = constructor(case)
    except (KeyError, ValueError, FileNotFoundError, TypeError, AssertionError) as error:
      return {'outcome': 'error', 'error': {'kind': type(error).__name__, 'message': str(error)}, 'parameter_reads': reads}
    if hasattr(state, 'updated_messages') or hasattr(state, 'rcp_tracks'):
      value = snapshot(state)
    else:
      common = {name: getattr(state, name) for name in ('frame', 'v_ego_hist', 'a_ego_hist', 'v_ego', 'a_ego',
        'last_timestamp', 'dt', 'init_samples', 'init_done')}
      common.update(pts={}, pts_order=[], tracks={}, tracks_order=[], history_maxlen=state.v_ego_hist.maxlen)
      value = normalized({'common': common, 'bits': floating_bits(common), 'backend': {}, 'parser': parser_snapshot(state.rcp)})
    return {'outcome': 'ok', 'state': value, 'warnings': [], 'parameter_reads': reads}


def cases(directory):
  from opendbc.car.car_helpers import interfaces
  from opendbc.car.interfaces import PLATFORMS
  output = []
  for candidate in sorted(interfaces):
    platform = PLATFORMS[candidate]
    for unavailable in (False, True):
      output.append({'name': f'constructor-{candidate}-unavailable{unavailable}', 'op': 'constructor',
        'candidate': candidate, 'delay': .02, 'period': .05, 'unavailable': unavailable,
        'flags': int(platform.config.flags), 'ext_flags': 0, 'safety_count': 1,
        'constructor_ns': 1_000_000_000, 'dbc_root': str(directory.resolve()),
        'params': {'EnableRadarTracks': '1', 'EnableCornerRadar': '0', 'HyundaiCameraSCC': '0'}})
  return output
