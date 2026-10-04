from __future__ import annotations


def ego(first=10, last=20, count=2, receive=30, velocity=12.5):
  return {'first_can_ns': first, 'last_can_ns': last, 'packet_count': count, 'receive_ns': receive, 'v_ego': velocity, 'a_ego': -1.2}


def can(*timestamps):
  return {'op': 'can', 'packets': [{'mono_time': timestamp, 'frames': [{'address': 291, 'data': [timestamp % 256], 'bus': 0}]}
                                 for timestamp in timestamps]}


def state(**arguments):
  return {'op': 'state', 'state': ego(**arguments)}


def take(now=31):
  return {'op': 'take', 'now': now}


def cases(*, include_parser=True):
  output = []
  def batch(name, actions):
    output.append({'name': name, 'op': 'batches', 'actions': actions})
  batch('state-before-can-and-partial-arrival', [state(), take(), can(1, 10), take(), can(20, 40), take(), take()])
  batch('can-before-state', [can(1, 10, 20, 40), take(), state(), take(), take()])
  batch('missing-middle-preserves-next-batch', [can(10, 20, 40), state(count=3), state(first=40, last=40, count=1, receive=50), take(51), take(51)])
  batch('exact-age-limit', [can(10, 20), state(), take(100_000_030)])
  batch('expired-metadata', [can(10, 20), state(), take(100_000_031)])
  batch('future-receive-clock', [can(10, 20), state(receive=1000), take(31)])
  for name, values in [('missing-metadata', {'receive': 0}), ('empty-valid', {'first': 0, 'last': 0, 'count': 0}),
                       ('empty-invalid', {'count': 0}), ('count-limit', {'count': 513}), ('zero-first', {'first': 0}),
                       ('reversed-endpoints', {'first': 20, 'last': 10})]:
    batch(name, [state(**values), take()])
  batch('state-overflow-precedes-other-errors', [*[state(receive=0) for _ in range(33)], take()])
  batch('can-capacity-drops-oldest', [can(*range(1, 523)), state(first=1, last=20, count=20), take()])
  batch('retain-corresponding-ego-not-latest', [state(velocity=12.5), state(first=40, last=40, count=1, receive=50, velocity=33.5),
        can(10, 20, 40), take(51), take(51)])
  for dt in (.01, .04, .05, .1):
    actions = [{'velocity': 15.0 + tick * -.07 + (.5 if tick == 30 else 0.0), 'stationary': 70 <= tick < 80} for tick in range(100)]
    actions += [{'velocity': 0.0, 'reset': True}, {'velocity': .1}]
    output.append({'name': f'lead-observer-{dt}', 'op': 'lead_filter', 'dt': dt, 'velocity': 15.0, 'actions': actions})
  for dt in (0.0, -0.05, float('nan'), float('inf'), 1e200):
    output.append({'name': f'lead-invalid-period-{dt}', 'op': 'lead_filter', 'dt': dt, 'velocity': 10.0, 'actions': []})
  output.append({'name': 'quadratic-pseudoinverse-weights', 'op': 'weights', 'counts': list(range(7, 53))})
  for velocity in (float('nan'), float('inf'), float('-inf')):
    output.append({'name': f'lead-nonfinite-velocity-{velocity}', 'op': 'lead_filter', 'dt': .05, 'velocity': 10.0,
                   'actions': [{'velocity': velocity}, {'velocity': 10.0}, {'velocity': 0.0, 'reset': True}]})
  for track_id, source in ((10, 'frontRadar'), (200, 'corner235'), (240, 'corner180'), (0, 'scc')):
    point = {'trackId': track_id, 'radarSource': source, 'dRel': 20.0, 'yRel': 1.0, 'vRel': -1.0,
             'vLead': 10.0, 'yvRel': .1, 'measured': True}
    actions = [{'point': {'vLead': 10.0 - tick * .05, 'dRel': 20.0 - tick * .02, 'yRel': 1.0 + tick * .01}}
               for tick in range(40)]
    actions += [{'point': {'dRel': 40.0, 'vRel': 10.0}}, {'point': {'measured': False}},
                {'point': {'measured': True, 'vLead': 0.0}}]
    output.append({'name': f'track-reset-jerk-{track_id}', 'op': 'track', 'track_id': track_id, 'dt': .05, 'point': point, 'actions': actions})
  for dt in (0.0, -.1, -.05, -.25, float('nan'), float('inf'), 1e-310):
    output.append({'name': f'track-invalid-period-{dt}', 'op': 'track', 'track_id': 10, 'dt': dt,
                   'point': point, 'actions': []})
  if include_parser:
    from radarcan_parser_source import cases as parser_cases
    output.extend(parser_cases())
  from radarcan_base_source import cases as base_cases
  output.extend(base_cases())
  return output
