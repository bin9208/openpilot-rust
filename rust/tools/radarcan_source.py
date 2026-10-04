from __future__ import annotations

from collections import deque
import contextlib
import io
import math
import struct


def normalized(value):
  import numpy as np
  if isinstance(value, np.generic):
    return normalized(value.item())
  if isinstance(value, float):
    return 'NaN' if math.isnan(value) else 'Infinity' if value == math.inf else '-Infinity' if value == -math.inf else value
  if isinstance(value, bytes):
    return list(value)
  if isinstance(value, (list, tuple, deque)):
    return [normalized(item) for item in value]
  if isinstance(value, dict):
    return {str(key): normalized(item) for key, item in value.items()}
  return value


def floating_bits(values):
  return {name: struct.pack('<d', float(value)).hex() for name, value in values.items() if isinstance(value, float)}


def batches(case):
  from openpilot.selfdrive.carrot.radar.can_batch import RadarCanBatches, RadarEgoSample
  state = RadarCanBatches()
  output = []
  for action in case['actions']:
    result = None
    if action['op'] == 'can':
      state.add_can([(packet['mono_time'], [(frame['address'], bytes(frame['data']), frame['bus'])
        for frame in packet['frames']]) for packet in action['packets']])
    elif action['op'] == 'state':
      state.add_state(RadarEgoSample(**action['state']))
    elif action['op'] == 'take':
      value = state.take(action['now'])
      if value is not None:
        ego, packets, error = value
        result = {'ego': vars(ego), 'packets': packets, 'error': error}
    else:
      raise ValueError(action['op'])
    output.append(normalized({'result': result, 'can': list(state.can),
      'states': [vars(ego) for ego in state.states], 'overflowed': state.overflowed}))
  return output


def lead_filter(case):
  from opendbc.car.radar_lead_filter import RadarLeadFilter
  output = []
  try:
    state = RadarLeadFilter(case['velocity'], case['dt'])
  except Exception as error:
    return {'error': {'kind': type(error).__name__, 'message': str(error)}}
  for action in case['actions']:
    if action.get('reset'):
      state.reset(action['velocity'])
    else:
      state.update(action['velocity'], stationary=action.get('stationary', False))
    output.append(normalized({'fields': vars(state).copy(), 'bits': floating_bits(vars(state))}))
  return output


def track(case):
  import numpy as np
  from opendbc.car import structs
  from opendbc.car.radar_tracks import MyTrack
  point = structs.RadarData.RadarPoint(**case['point'])
  try:
    state = MyTrack(case['track_id'], point, case['dt'])
  except Exception as error:
    return normalized({'point': point.to_dict(), 'error': {'kind': type(error).__name__, 'message': str(error)}})
  output = []
  for action in case['actions']:
    for key, value in action['point'].items():
      setattr(point, key, value)
    state.update(point, action.get('a_ego', 0.0))
    state.write_acceleration(point)
    fields = {name: value for name, value in vars(state).items()
      if isinstance(value, (str, int, float, bool, deque, np.generic))}
    for name in ('vLead_avg', 'aLead_avg', 'jLead_avg', 'yRel_avg', 'yvRel_avg', 'lead_filter'):
      fields[name] = vars(getattr(state, name)).copy()
    output.append(normalized({'point': point.to_dict(), 'point_bits': floating_bits(point.to_dict()),
      'fields': fields, 'bits': floating_bits(vars(state))}))
  return output


def trace(case):
  from radarcan_parser_source import trace as parser_sets
  from radarcan_base_source import trace as base
  from radarcan_decoder_source import trace as decoder
  from radarcan_cluster_source import trace as cluster
  from radarcan_loop_source import trace as runtime
  from radarcan_constructor_source import trace as constructor
  functions = {'batches': batches, 'lead_filter': lead_filter, 'track': track, 'weights': weights,
               'parser_sets': parser_sets, 'base': base, 'decoder': decoder, 'cluster': cluster, 'runtime': runtime,
               'constructor': constructor}
  stdout, stderr = io.StringIO(), io.StringIO()
  with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
    result = functions[case['op']](case)
  return {'name': case['name'], 'result': result, 'stdout': stdout.getvalue(), 'stderr': stderr.getvalue()}


def weights(case):
  from opendbc.car.radar_tracks import radar_quadratic_jerk_weights
  output = {}
  for count in case['counts']:
    values = radar_quadratic_jerk_weights(count)
    output[str(count)] = {'weights': values, 'bits': [struct.pack('<d', value).hex() for value in values]}
  return normalized(output)
