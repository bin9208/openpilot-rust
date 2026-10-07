"""Run both original planners and compare complete state to the Rust owners."""

import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

from plannerd_owner_fixtures import SERVICES, car_params, messages, parameters, save
from plannerd_owner_loader import SourceOwners
from plannerd_owner_snapshot import frame as snapshot


def difference(expected, actual, path='', records=None):
  if records is None:
    records = []
  if isinstance(expected, dict):
    if expected.keys() != actual.keys():
      records.append({'path': path, 'expected_keys': sorted(expected), 'actual_keys': sorted(actual)})
      return records
    for key, value in expected.items():
      difference(value, actual[key], path + '.' + key, records)
  elif isinstance(expected, list):
    if len(expected) != len(actual):
      records.append({'path': path, 'expected_length': len(expected), 'actual_length': len(actual)})
      return records
    for index, (value, other) in enumerate(zip(expected, actual, strict=True)):
      difference(value, other, f'{path}[{index}]', records)
  elif isinstance(expected, float):
    if actual is None or struct.pack('<d', expected) != struct.pack('<d', float(actual)):
      records.append(
        {
          'path': path,
          'expected': expected,
          'actual': actual,
          'expected_hex': expected.hex(),
          'actual_hex': float(actual).hex() if actual is not None else None,
        }
      )
  elif expected != actual:
    records.append({'path': path, 'expected': expected, 'actual': actual})
  return records


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source-native', type=Path, required=True)
  parser.add_argument('--artifact', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--frames', type=int, default=360)
  args = parser.parse_args()
  args.output = args.output.resolve()
  args.output.mkdir(parents=True, exist_ok=False)
  values = parameters()
  source = SourceOwners(args.source_native.resolve(), values)
  cp = car_params()
  cp_path = save(args.output, cp)
  long = source.long.LongitudinalPlanner(cp)
  lat = source.lat.LateralPlanner(cp, debug=False)
  carrot = source.carrot.CarrotPlanner()
  sm = source.messaging.SubMaster(SERVICES, poll=['modelV2', 'liveTracks'], ignore_avg_freq=['radarState'])
  frames, expected = [], []
  for index in range(args.frames):
    source.now = 100.0 + index * 0.05
    source.wall = 1000.0 + index * 0.05
    changes = {
      100: {'LeadAccelResponse': '5', 'TFollowDecelBoost': '70', 'SpeedTFFactor': '14'},
      240: {'MyDrivingMode': '4'},
      300: {'MyDrivingMode': '2'},
      330: {'MyDrivingMode': '3'},
    }.get(index, {})
    source.store.values.update({key: value.encode() for key, value in changes.items()})
    packets = messages(source.messaging.new_message, index, source.now)
    sm.update_msgs(source.now, [value.as_reader() for value in packets.values()])
    navigation = source.navigation.get_carrot_man(sm)
    checks = {
      'mode': sm.all_checks(['carState', 'radarState']),
      'lane': all(sm.valid[key] and sm.alive[key] for key in ('carState', 'modelV2', 'radarState')),
      'pose': sm.valid['livePose'] and sm.alive['livePose'],
      'coasting': sm.all_checks(['carState', 'controlsState', 'selfdriveState', 'radarState', 'modelV2']),
      'navigation_seen': sm.seen['carrotMan'],
      'navigation_valid': navigation is not None,
    }
    frames.append(
      {
        'time': source.now,
        'wall_time': source.wall,
        'messages': {key: save(args.output, packet) for key, packet in packets.items()},
        'parameters': changes,
        'checks': checks,
      }
    )
    source.logs.clear()
    long.update(sm, carrot)
    lat.update(sm, carrot)
    expected.append(snapshot(long, lat, carrot, source.logs, source.store.operations))
    source.store.operations.clear()
  request = {'artifact': str(args.artifact.resolve()), 'car_params': cp_path, 'parameters': values, 'frames': frames}
  data = json.dumps(request, allow_nan=False).encode()
  (args.output / 'input.json').write_bytes(data)
  (args.output / 'expected.json').write_text(json.dumps(expected, allow_nan=False) + '\n')
  result = subprocess.run([str(args.binary.resolve())], input=data, capture_output=True, check=False)
  (args.output / 'actual.json').write_bytes(result.stdout)
  (args.output / 'stderr.txt').write_bytes(result.stderr)
  result.check_returncode()
  actual = json.loads(result.stdout)
  mismatches = difference(expected, actual)
  receipt = {
    'frames': len(expected),
    'mismatch_count': len(mismatches),
    'first_mismatches': mismatches[:100],
    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    'source_files': {
      str(path): hashlib.sha256(path.read_bytes()).hexdigest()
      for path in [
        Path('openpilot/selfdrive/controls/lib/longitudinal_planner.py'),
        Path('openpilot/selfdrive/controls/lib/lateral_planner.py'),
        Path('openpilot/selfdrive/carrot/carrot_functions.py'),
        Path('openpilot/selfdrive/controls/lib/longitudinal_mpc_lib/long_mpc.py'),
      ]
    },
  }
  (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
  assert not mismatches, f'{len(mismatches)} differences: {mismatches[:2]}'
  print(json.dumps({'frames': len(expected), 'exact': True}))


if __name__ == '__main__':
  main()
