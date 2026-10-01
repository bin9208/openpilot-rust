#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess
from openpilot.cereal import log


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  packets = []
  for service in ['deviceState', 'carState', 'controlsState', 'carControl', 'livePose', 'liveCalibration', 'wideRoadCameraState', 'liveDelay', 'managerState', 'navInstruction']:
    packet = log.Event.new_message()
    packet.init(service)
    packet.logMonoTime = 123456789
    packets.append(packet)
  packets[0].deviceState.networkMetered = True
  packets[0].deviceState.cpuTempC = [1.0, 2.5]
  packets[5].liveCalibration.rpyCalib = [0.0, 0.1, 0.2]
  packet = log.Event.new_message()
  packet.init('can', 1)
  packet.can[0].address = 123
  packet.can[0].dat = b'\x00\xff'
  packets.append(packet)
  process = subprocess.run([args.binary, 'decode'], input=''.join(json.dumps(list(packet.to_bytes())) + '\n' for packet in packets), text=True, capture_output=True, check=True)
  native = [json.loads(line) for line in process.stdout.splitlines()]
  records = []
  for packet, actual in zip(packets, native, strict=True):
    try:
      expected = {'result': json.loads(json.dumps(packet.to_dict()))}
    except TypeError as error:
      expected = {'error': str(error)}
    records.append({'service': packet.which(), 'source': expected, 'native': actual, 'pass': expected == actual})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  assert all(row['pass'] for row in records), [row['service'] for row in records if not row['pass']]
  print(f'PASS: {len(records)} complete cereal Event-to-dict/JSON cases including source bytes serialization failure')


if __name__ == '__main__':
  main()
