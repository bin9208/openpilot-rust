#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = ["pytest", "numpy", "pycapnp"]
# ///
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import struct
import time
import uuid

from joystickd_source import load_binding


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--daemon', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  load_binding(args.binding.resolve())
  from openpilot.cereal import messaging

  request = (args.source / 'input.json').read_bytes()
  inputs = json.loads(request)['inputs']
  expected = json.loads((args.source / 'expected.json').read_bytes())
  trace = subprocess.run([str(args.trace.resolve())], input=request, capture_output=True, check=False)
  (args.output / 'trace.stdout.json').write_bytes(trace.stdout)
  (args.output / 'trace.stderr.log').write_bytes(trace.stderr)
  trace.check_returncode()
  actual = json.loads(trace.stdout)
  assert len(actual) == len(expected)

  def decode(packet: bytes) -> dict:
    message = messaging.log_from_bytes(packet)
    name = message.which()
    return {'service': name, 'valid': message.valid, 'data': getattr(message, name).to_dict()}

  mismatches = []
  max_acceleration_error = 0.0
  for index, (source, native) in enumerate(zip(expected, actual, strict=True)):
    messages = [decode(bytes(packet)) for packet in native['packets']]
    row = {'messages': messages, **{name: native[name] for name in ('acceleration_bits', 'baseline_bits', 'selected', 'state', 'complete_remaining', 'display_holdoff')}}
    source_accel = struct.unpack('=d', struct.pack('=Q', source['acceleration_bits']))[0]
    native_accel = struct.unpack('=d', struct.pack('=Q', row['acceleration_bits']))[0]
    acceleration_error = abs(source_accel - native_accel)
    max_acceleration_error = max(max_acceleration_error, acceleration_error)
    same = {key: value for key, value in source.items() if key != 'acceleration_bits'} == {key: value for key, value in row.items() if key != 'acceleration_bits'}
    if not same or acceleration_error > 1e-15:
      mismatches.append({'step': index, 'expected': source, 'actual': row})
  policy = {'steps': len(actual), 'mismatch_count': len(mismatches), 'mismatches': mismatches[:3],
    'raw_acceleration_absolute_tolerance': 1e-15, 'max_raw_acceleration_error': max_acceleration_error,
    'trace_sha256': hashlib.sha256(args.trace.read_bytes()).hexdigest(), 'command': [str(args.trace.resolve())]}
  (args.output / 'comparison.json').write_text(json.dumps(policy, indent=2) + '\n')
  assert not mismatches, (len(mismatches), mismatches[:1])

  os.environ['OPENPILOT_PREFIX'] = 'lat_maneuvers_' + uuid.uuid4().hex
  os.environ['PARAMS_ROOT'] = str((args.output / 'params').resolve())
  os.environ['SIMULATION'] = '1'
  params = Path(os.environ['PARAMS_ROOT']) / os.environ['OPENPILOT_PREFIX']
  params.mkdir(parents=True)
  cp = params / 'CarParams'
  cp.mkdir()
  ipc = Path('/dev/shm') / ('msgq_' + os.environ['OPENPILOT_PREFIX'])
  ipc.mkdir()
  names = ['carState', 'carControl', 'controlsState', 'selfdriveState', 'modelV2']
  publisher = messaging.PubMaster(names)
  subscribers = [messaging.sub_sock(name) for name in ['alertDebug', 'lateralManeuverPlan']]
  stdout = (args.output / 'daemon.stdout.log').open('w')
  stderr = (args.output / 'daemon.stderr.log').open('w')
  daemon_command = [str(args.daemon.resolve()), '--frames', str(len(inputs))]
  child = subprocess.Popen(daemon_command, stdout=stdout, stderr=stderr)
  captures, times = [], []

  def receive() -> list[dict]:
    packets = [None] * len(subscribers)
    deadline = time.monotonic() + 3
    while any(packet is None for packet in packets):
      for index, subscriber in enumerate(subscribers):
        if packets[index] is None:
          packets[index] = subscriber.receive(non_blocking=True)
      if any(packet is None for packet in packets):
        assert time.monotonic() < deadline and child.poll() is None, 'daemon publication timeout'
        time.sleep(.001)
    messages = []
    for packet in packets:
      event = messaging.log_from_bytes(packet)
      assert event.logMonoTime > 0
      times.append(event.logMonoTime)
      messages.append(decode(packet))
    captures.append(messages)
    return messages

  def publish(row: dict) -> None:
    packets = [messaging.new_message(name) for name in names]
    for packet in packets:
      packet.valid = True
    car = packets[0].carState
    car.vEgo, car.steeringPressed = row['speed'], row['steering_pressed']
    packets[1].carControl.latActive = row['active']
    packets[1].carControl.orientationNED = row['orientation']
    packets[2].controlsState.desiredCurvature = row['curvature']
    packets[2].valid = row['valid']
    for name, packet in zip(names, packets, strict=True):
      publisher.send(name, packet)

  try:
    time.sleep(.15)
    assert not subscribers[0].receive(non_blocking=True)
    assert child.poll() is None
    cp.rmdir()
    cp.write_bytes((args.source / 'CarParams.bin').read_bytes())
    assert receive() == expected[0]['messages'], 'initial model absence/default values'
    for index, row in enumerate(inputs[1:], 1):
      publish(row)
      received = receive()
      assert received == expected[index]['messages'], (index, received, expected[index]['messages'])
    assert child.wait(timeout=3) == 0
  finally:
    if child.poll() is None:
      child.send_signal(signal.SIGTERM)
      child.wait(timeout=3)
    stdout.close()
    stderr.close()
    (args.output / 'ipc.publications.json').write_text(json.dumps(captures) + '\n')

  exits = {}
  cp.unlink()
  for name, raw in [('waiting', None), ('malformed', b'bad cereal')]:
    if raw is not None:
      cp.write_bytes(raw)
    with (args.output / (name + '.stdout.log')).open('w') as out, (args.output / (name + '.stderr.log')).open('w') as err:
      process = subprocess.Popen([str(args.daemon.resolve())], stdout=out, stderr=err)
      if raw is None:
        time.sleep(.1)
        assert process.poll() is None
        process.send_signal(signal.SIGTERM)
      code = process.wait(timeout=3)
      assert code == (0 if raw is None else 1)
      exits[name] = code
  invalid = subprocess.run([str(args.daemon.resolve()), '--frames', '0'], capture_output=True, check=False)
  assert invalid.returncode == 1
  (args.output / 'invalid-frames.stderr.log').write_bytes(invalid.stderr)
  receipt = {'steps': len(actual), 'mismatch_count': 0, 'ipc_publications_per_topic': len(captures),
    'presets_selected': len({row['selected'] for row in actual if row['selected'] is not None}),
    'all_six_completed': actual[-1]['selected'] is None, 'initial_model_timeout': True,
    'invalid_input_does_not_gate_active_plan': any(not row['valid'] and output['messages'][1]['valid'] for row, output in zip(inputs, expected, strict=True)),
    'holdoff_final_baseline_valid': sum(row['state'] is not None and row['state']['finished'] and row['complete_remaining'] == 0 and row['messages'][1]['valid'] for row in expected), 'exit_codes': exits,
    'daemon_sha256': hashlib.sha256(args.daemon.read_bytes()).hexdigest(), 'command': daemon_command,
    'simulation': True, 'prefix': os.environ['OPENPILOT_PREFIX']}
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
