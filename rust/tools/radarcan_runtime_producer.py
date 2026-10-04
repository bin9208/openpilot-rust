from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import time

from radarcan_runtime_types import Action, Case


def prepare(action: Action):
  from openpilot.cereal import messaging
  packets = []
  for packet in action['packets']:
    event = messaging.new_message('can', len(packet['frames']), valid=True, logMonoTime=packet['mono_time'])
    for index, frame in enumerate(packet['frames']):
      event.can[index] = {'address': frame['address'], 'dat': bytes(frame['data']), 'src': frame['bus']}
    packets.append(event)
  state = messaging.new_message('carState', valid=True)
  state.carState.vEgo, state.carState.aEgo = action['v_ego'], action['a_ego']
  state.carState.radarInput = action.get('metadata', {'firstCanMonoTime': action['packets'][0]['mono_time'] if packets else 0,
    'lastCanMonoTime': action['packets'][-1]['mono_time'] if packets else 0, 'canPacketCount': len(packets),
    'receiveMonoTime': int(round(action['time'] * 1e9))})
  return packets, state


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--case', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--prefix', action='append', required=True)
  parser.add_argument('--ready', type=Path, required=True)
  parser.add_argument('--publishers-ready', type=Path, required=True)
  parser.add_argument('--start', type=Path, required=True)
  parser.add_argument('--prequeue-complete', type=Path)
  parser.add_argument('--order', choices=('can-state', 'state-can'), default='can-state')
  args = parser.parse_args()
  from openpilot.cereal import messaging
  case: Case = json.loads(args.case.read_text())
  prepared = [prepare(action) for action in case['actions']]
  publishers = []
  for prefix in args.prefix:
    os.environ['OPENPILOT_PREFIX'] = prefix
    publishers.append(messaging.PubMaster(['can', 'carState']))
  args.publishers_ready.write_text(json.dumps({'pid': os.getpid(), 'ready_ns': time.monotonic_ns()}) + '\n')
  deadline = time.monotonic() + 10
  while not all(publisher.all_readers_updated(topic) for publisher in publishers for topic in ('can', 'carState')):
    if time.monotonic() >= deadline:
      raise TimeoutError('radarcan input subscribers did not connect')
    time.sleep(.001)
  args.ready.write_text(json.dumps({'pid': os.getpid(), 'ready_ns': time.monotonic_ns()}) + '\n')
  while not args.start.exists():
    if time.monotonic() >= deadline:
      raise TimeoutError('radarcan parent did not release the prepared producer')
    time.sleep(.0005)
  rows = []
  first = case['actions'][0]['time']
  origin = time.monotonic_ns() + 30_000_000
  offset = origin - int(round(first * 1e9))
  encoded = []
  for packets, state in prepared:
    for packet in packets:
      if packet.logMonoTime:
        packet.logMonoTime += offset
    metadata = state.carState.radarInput
    if metadata.receiveMonoTime:
      metadata.receiveMonoTime += offset
    if metadata.firstCanMonoTime:
      metadata.firstCanMonoTime += offset
    if metadata.lastCanMonoTime:
      metadata.lastCanMonoTime += offset
    encoded.append(([packet.to_bytes() for packet in packets], state.to_bytes()))
  encoded_done = time.monotonic_ns()
  for index, (action, (packets, state)) in enumerate(zip(case['actions'], encoded, strict=True)):
    target = origin + int(round((action['time'] - first) * 1e9))
    remaining = target - time.monotonic_ns()
    if remaining > 0:
      time.sleep(remaining * 1e-9)
    before = time.monotonic_ns()
    state_count = action.get('state_count', 1)
    if args.order == 'state-can':
      for publisher in publishers:
        for _ in range(state_count):
          publisher.send('carState', state)
    can_times = []
    for packet in packets:
      if action.get('can_delay_ms', 0) > 0:
        time.sleep(action['can_delay_ms'] / 1000)
      for publisher in publishers:
        publisher.send('can', packet)
      can_times.append(time.monotonic_ns())
    if args.order == 'can-state':
      for publisher in publishers:
        for _ in range(state_count):
          publisher.send('carState', state)
    rows.append({'scheduled_ns': target, 'send_started_ns': before, 'send_finished_ns': time.monotonic_ns(),
      'packet_count': len(packets), 'state_count': state_count, 'can_sent_ns': can_times})
    if index == 0 and args.prequeue_complete is not None:
      args.prequeue_complete.write_text(str(time.monotonic_ns()))
  deadline = time.monotonic() + .05
  while not all(publisher.all_readers_updated(topic) for publisher in publishers for topic in ('can', 'carState')):
    if time.monotonic() >= deadline:
      raise TimeoutError('final radarcan input was not acknowledged within 50 ms')
    time.sleep(.0005)
  final_ack = time.monotonic_ns()
  input_rows = [{'can': [packet.hex() for packet in packets], 'carState': state.hex()}
    for packets, state in encoded]
  inputs_path = args.output.with_name('inputs.json')
  inputs_path.write_text(json.dumps(input_rows) + '\n')
  args.output.write_text(json.dumps({'pid': os.getpid(), 'rows': rows, 'order': args.order,
    'origin_ns': origin, 'offset_ns': offset, 'encoded_done_ns': encoded_done,
    'final_ack_ns': final_ack, 'evidence_done_ns': time.monotonic_ns(), 'prefixes': args.prefix,
    'inputs_sha256': hashlib.sha256(inputs_path.read_bytes()).hexdigest()}) + '\n')


if __name__ == '__main__':
  main()
