#!/usr/bin/env python3
"""Compare unchanged timed.main's local datetime domain using safe GPS policy fixtures."""
import argparse
import json
from pathlib import Path
import tempfile

from check_timed_reference import record_view, step
from timed_fixtures import environment, native, normalized
from timed_reference import Source


def case(binary, output, zone, milliseconds):
  output.mkdir(parents=True, exist_ok=True)
  results = []
  for implementation in ['python', 'rust']:
    with tempfile.TemporaryDirectory(prefix='timed-date-') as temporary, environment(Path(temporary), zone) as (config, params):
      (params / 'TimezoneSource').write_text('app')
      action = step(10_000_000_000, unix_timestamp_millis=milliseconds)
      config['actions'] = [action]
      if implementation == 'python':
        source = Source(config, params)
        error = None
        try:
          source.loop([action])
        except (ValueError, OverflowError, OSError) as failure:
          error = str(failure)
        observed = normalized(config, params, source.records())
        result = {'error': error, 'sleeps': source.sleeps, 'state': observed}
      else:
        rows, records = native(binary, config, output / 'native.jsonl')
        observed = normalized(config, params, record_view(records))
        result = {'error': rows[0]['result'].get('error'), 'sleeps': rows[0]['sleeps'], 'state': observed}
      results.append(result)
  passed = (results[0]['error'] is None) == (results[1]['error'] is None)
  passed = passed and results[0]['sleeps'] == results[1]['sleeps'] and results[0]['state'] == results[1]['state']
  assert not results[0]['state']['commands'] and not results[1]['state']['commands']
  report = {'zone': zone, 'unix_timestamp_millis': milliseconds, 'passed': passed, 'results': results}
  (output / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  offsets = [-86400001, -86400000, -50400001, -50400000, -43200001, -43200000, -1, 0, 1,
             43200000, 43200001, 50400000, 50400001, 86400000]
  for boundary in [36000000, 86400000, 129600000, 55928000, 104162000, -32400000, 18000000]:
    offsets.extend([boundary - 1, boundary, boundary + 1])
  offsets = sorted(set(offsets))
  epochs = [center + offset for center in [-62135596800000, 253402300800000] for offset in offsets]
  epochs += [-62167219200000, -62167219200001, -62198755200000, 0]
  results = []
  for zone in ['UTC', 'Etc/GMT-14', 'Etc/GMT+12', 'Asia/Seoul', 'America/New_York']:
    for milliseconds in epochs:
      name = zone.replace('/', '-') + '-' + str(milliseconds)
      results.append(case(args.binary, args.output / name, zone, milliseconds))
  failures = [result for result in results if not result['passed']]
  report = {'passed': not failures, 'scenarios': len(results), 'failures': failures, 'results': results}
  (args.output / 'summary.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': not failures, 'scenarios': len(results), 'failed_cases': len(failures)}, indent=2))
  assert not failures, [(row['zone'], row['unix_timestamp_millis']) for row in failures]


if __name__ == '__main__':
  main()
