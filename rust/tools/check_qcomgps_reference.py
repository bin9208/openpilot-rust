#!/usr/bin/env python3
"""Compare all five diagnostic GNSS reports with unchanged source publication bodies."""
import argparse
import json
from pathlib import Path
import subprocess

from openpilot.cereal import log
from qcomgps_reference import Source, packet, payload


def scenarios():
  for seed in range(4):
    for kind, header, satellite in [(0x1477, 'gps_measurement_report', 'gps_measurement_report_sv'),
                                    (0x1480, 'glonass_measurement_report', 'glonass_measurement_report_sv'),
                                    (0x14de, 'oemdre_measurement_report', 'oemdre_measurement_report_sv')]:
      for count in [0, 1, 3]:
        report = payload(header, {'version': 2 if kind == 0x14de else 0, 'sv_count': count, 'source': seed % 2}, seed)
        satellites = b''.join(payload(satellite, {'observation_state': index % 3, 'measurement_status': 0xffffffff if seed % 2 else 0,
            'misc_status': seed, 'multipath_estimate_valid': seed % 2, 'direction_valid': seed % 2}, seed) for index in range(count))
        yield f'measurements-{kind:x}-{seed}-{count}', 16, packet(kind, report + satellites, seed)
        yield f'version-{kind:x}-{seed}-{count}', 16, packet(kind, b'\xff' + report[1:] + satellites)
        if count:
          yield f'truncated-{kind:x}-{seed}-{count}', 16, packet(kind, report + satellites[:-1])
    yield f'polynomial-{seed}', 16, packet(0x14e1, payload('oemdre_svpoly_report', {'version': 2}, seed))
    yield f'polynomial-version-{seed}', 16, packet(0x14e1, payload('oemdre_svpoly_report', {'version': 1}, seed))
    for week in [0, 2440, 65534, 65535]:
      for vdop in [0., 499.99997, 500., 501.]:
        report = payload('position_report', {'u_PosSource': 2, 'w_GpsWeekNumber': week, 'q_FltVdop': vdop,
            'q_GpsFixTimeMs': 123456789, 'q_FltHeadingUncRad': 0 if seed % 2 else .05}, seed)
        yield f'position-{seed}-{week}-{vdop}', 16, packet(0x1476, report)
    yield f'position-source-{seed}', 16, packet(0x1476, payload('position_report', {'u_PosSource': 1}, seed))
  for millis in [0, 1, 17999, 18000, 18001, 604800000, 4294967295]:
    for week in [0, 2440]:
      report = payload('position_report', {'u_PosSource': 2, 'w_GpsWeekNumber': week, 'q_GpsFixTimeMs': millis})
      yield f'position-timestamp-{week}-{millis}', 16, packet(0x1476, report)
  yield 'unknown-log', 16, packet(0xffff, b'')
  yield 'unknown-opcode', 115, b'ignored'
  for length in range(15):
    yield f'short-envelope-{length}', 16, b'\0' * length
  yield 'length-mismatch', 16, packet(0x1477, b'') + b'\0'


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  cases = list(scenarios())
  rows = [{'opcode': opcode, 'payload': list(data), 'mono': 123456789} for _, opcode, data in cases]
  completed = subprocess.run([args.binary], input=''.join(json.dumps(row) + '\n' for row in rows), text=True,
                             capture_output=True, timeout=30, check=True)
  actual = [json.loads(line) for line in completed.stdout.splitlines()]
  assert len(actual) == len(cases)
  source = Source()
  records = []
  for (name, opcode, data), result in zip(cases, actual, strict=True):
    expected = source.publication(opcode, data)
    if 'data' in result:
      with log.Event.from_bytes(bytes(result.pop('data'))) as event:
        result['event'] = event.to_dict()
      has_fix = result.pop('has_fix')
      assert has_fix == (result['event'].get('gpsLocation', {}).get('hasFix', False)), name
    if 'error' in expected:
      assert 'error' in result, (name, expected, result)
    else:
      assert result == expected, (name, expected, result)
    records.append({'case': name, 'source': expected, 'native': result, 'pass': True})
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(records, indent=2) + '\n')
  print(f'PASS: {len(records)} source/native GNSS publication comparisons')


if __name__ == '__main__':
  main()
