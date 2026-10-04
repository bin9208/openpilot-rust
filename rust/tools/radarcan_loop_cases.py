from __future__ import annotations

import contextlib
import copy
import io

from radarcan_decoder_cases import cases as decoder_cases


def ego(packet, receive=None):
  return {'first_can_ns': packet['mono_time'], 'last_can_ns': packet['mono_time'], 'packet_count': 1,
    'receive_ns': packet['mono_time'] if receive is None else receive, 'v_ego': 12., 'a_ego': -.2}


def tick(now, packets=(), states=()):
  return {'now_ns': now, 'can': list(packets), 'carState': list(states)}


def scenario(base, name, ticks):
  output = {key: copy.deepcopy(value) for key, value in base.items() if key not in ('actions', 'parser')}
  output.update(name='runtime-' + name, op='runtime', ticks=ticks)
  return output


def cases(directory):
  with contextlib.redirect_stdout(io.StringIO()):
    base = next(case for case in decoder_cases(directory) if case['name'] == 'decoder-VOLKSWAGEN_ID4_MK1-lifecycle')
  packet = base['actions'][0]['packets'][0]

  def at(when):
    item = copy.deepcopy(packet)
    item['mono_time'] = when
    return item

  initial = 1_000_000_000
  joined = [tick(initial + index * 40_000_000, [at(initial + index * 40_000_000)], [ego(at(initial + index * 40_000_000))])
    for index in range(8)]
  output = [scenario(base, 'healthy', joined),
    scenario(base, 'state-before-can', [tick(initial, states=[ego(packet)]), tick(initial + 10_000_000, [packet])]),
    scenario(base, 'can-before-state', [tick(initial, [packet]), tick(initial + 10_000_000, states=[ego(packet)])])]
  last = at(initial + 2)
  associated = ego(packet)
  associated.update(last_can_ns=last['mono_time'], packet_count=3)
  recovered = at(initial + 60_000_000)
  output.append(scenario(base, 'missing-can-recreates', [tick(initial + 2, [packet, last], [associated]),
    tick(recovered['mono_time'], [recovered], [ego(recovered)])]))
  empty = {'first_can_ns': 0, 'last_can_ns': 0, 'packet_count': 0, 'receive_ns': initial + 100_000_001, 'v_ego': 12., 'a_ego': -.2}
  recovered = at(initial + 102_000_000)
  output.append(scenario(base, 'can-timeout-recreates', [tick(initial, [packet], [ego(packet)]),
    tick(empty['receive_ns'], states=[empty]), tick(recovered['mono_time'], [recovered], [ego(recovered)])]))
  output.append(scenario(base, 'state-overflow', [tick(initial, states=[dict(empty, receive_ns=initial) for _ in range(33)])]))
  output.append(scenario(base, 'stale-state', [tick(initial + 100_000_001, [packet], [ego(packet)])]))
  output.append(scenario(base, 'input-timeout-throttle', [tick(initial + delta) for delta in [100_000_001, 110_000_000, 151_000_000]]))
  output.append(scenario(base, 'backward-clock-error-throttle', [tick(1, states=[dict(empty, receive_ns=0)]), tick(initial + 60_000_000)]))
  processing = tick(initial, [packet], [ego(packet)])
  processing['processing_ns'] = 100_000_001
  output.append(scenario(base, 'processing-timeout', [processing]))
  output.append(scenario(base, 'malformed-can-is-atomic', [tick(initial, [packet, [0]], [ego(packet)])]))
  output.append(scenario(base, 'malformed-state-preserves-earlier-state', [tick(initial, [packet], [ego(packet), [0]])]))
  flipped = scenario(base, 'flipped-publication-keeps-raw-history', joined)
  flipped['flip'] = True
  output.append(flipped)
  replay = scenario(base, 'replay-idle-does-not-advance', [tick(initial + 101_000_000), tick(initial + 900_000_000)])
  replay['replay'] = True
  output.append(replay)
  return output
