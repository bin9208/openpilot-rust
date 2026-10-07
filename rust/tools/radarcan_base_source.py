from __future__ import annotations


def data(value):
  if value is None:
    return None
  return {'points': [point.to_dict() for point in value.points], 'errors': value.errors.to_dict(),
    'radarTrackFlipped': bool(value.radarTrackFlipped), 'canMonoTimesDEPRECATED': list(value.canMonoTimesDEPRECATED),
    'errorsDEPRECATED': [str(error) for error in value.errorsDEPRECATED]}


def track_fields(state):
  from collections import deque
  import numpy as np
  fields = {name: value for name, value in vars(state).items() if isinstance(value, (str, int, float, bool, deque, np.generic))}
  for name in ('vLead_avg', 'aLead_avg', 'jLead_avg', 'yRel_avg', 'yvRel_avg', 'lead_filter'):
    fields[name] = vars(getattr(state, name)).copy()
  return fields


def trace(case):
  from opendbc.car import structs
  from opendbc.car.interfaces import RadarInterfaceBase
  from radarcan_source import normalized, floating_bits

  class Fixture(RadarInterfaceBase):
    def update(self, packets):
      action = self.action
      if action.get('fallback'):
        return super().update(packets)
      for address in action.get('delete', []):
        self.pts.pop(address, None)
      for address, values in action.get('points', []):
        if address not in self.pts:
          self.pts[address] = structs.RadarData.RadarPoint()
        for name, value in values.items():
          setattr(self.pts[address], name, value)
      if action.get('none'):
        return None
      selected = action.get('selected', list(self.pts))
      return structs.RadarData(points=[self.pts[address] for address in selected])

  cp = structs.CarParams(radarDelay=case['delay'], radarTimeStep=case['period'])
  try:
    state = Fixture(cp)
  except Exception as error:
    return {'constructor_error': {'kind': type(error).__name__, 'message': str(error)}}
  output = []
  for action in case['actions']:
    state.action = action
    result, error = None, None
    try:
      result = state.update_carrot(action['v_ego'], action['a_ego'], action['time'], [])
    except Exception as caught:
      error = {'kind': type(caught).__name__, 'message': str(caught)}
    snapshot = {name: getattr(state, name) for name in ('frame', 'v_ego_hist', 'a_ego_hist', 'v_ego', 'a_ego',
      'last_timestamp', 'dt', 'init_samples', 'init_done')}
    snapshot['pts'] = {str(address): point.to_dict() for address, point in state.pts.items()}
    snapshot['pts_order'] = list(state.pts)
    snapshot['tracks'] = {str(track_id): track_fields(track) for track_id, track in state.tracks.items()}
    snapshot['tracks_order'] = list(state.tracks)
    snapshot['history_maxlen'] = state.v_ego_hist.maxlen
    output.append(normalized({'result': data(result), 'error': error, 'state': snapshot,
      'bits': floating_bits(snapshot)}))
  return output


def cases():
  def action(tick, **values):
    return {'v_ego': 10.0 + tick * .01, 'a_ego': -.5 + tick * .001, 'time': 100.0 + tick * .05, **values}
  output = []
  for delay in (0.0, .8):
    output.append({'name': f'base-fallback-history-{delay}', 'op': 'base', 'delay': delay, 'period': .05,
      'actions': [action(tick, fallback=True) for tick in range(100)]})
  point = {'trackId': 10, 'dRel': 20.0, 'yRel': 1.0, 'vRel': -.5, 'vLead': 10.0, 'measured': True}
  output.append({'name': 'base-estimated-period-nonzero-jerk', 'op': 'base', 'delay': 0.0, 'period': 0.0,
    'actions': [action(tick, points=[[10, {**point, 'vLead': 10.0 + tick * .01 - .0005 * tick**2}]]) for tick in range(180)]})
  for period in (.04, .05, 0.0):
    actions = [action(tick, points=[[10, {**point, 'yRel': 1.0 + tick * .01, 'vLead': 10.0 - tick * .02}]])
               for tick in range(140)]
    actions.extend([action(140, points=[[20, {**point, 'trackId': 200}]], selected=[20]),
      action(141, delete=[10], points=[[20, {'dRel': 40.0}]]), action(142, points=[[20, {'measured': False}]]),
      action(143, points=[[20, {'trackId': 201, 'measured': True}]]), action(144, none=True)])
    output.append({'name': f'base-selected-copy-track-lifecycle-{period}', 'op': 'base', 'delay': .02,
      'period': period, 'actions': actions})
  for delay in (-.01, -.02, float('nan'), float('inf'), 1e30, -1e30):
    output.append({'name': f'base-invalid-delay-{delay}', 'op': 'base', 'delay': delay, 'period': .05,
      'actions': [action(0, fallback=True)]})
  for period in (float('inf'), float('nan')):
    output.append({'name': f'base-nonfinite-period-{period}', 'op': 'base', 'delay': 0.0, 'period': period,
      'actions': [action(tick, points=[[10, point]]) for tick in range(103)]})
  return output
