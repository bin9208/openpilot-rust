#!/usr/bin/env python3
"""Original beep methods and real Cython integer reads against the native fixture."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import zmq
from beep_fixtures import environment, lines


def execute(binary, binding, output, name, settings, volume):
  results = []
  for implementation in ['python', 'rust']:
    with tempfile.TemporaryDirectory(prefix='beep-policy-') as temporary, environment(Path(temporary), binding, volume) as (config, _params):
      config.update(settings)
      config['trace'] = str(output / f'{name}-{implementation}.trace.jsonl')
      context = zmq.Context()
      collector = context.socket(zmq.PULL)
      collector.bind('ipc://' + str(Path(temporary) / 'swaglog'))
      arguments = [sys.executable, str(Path(__file__).with_name('beep_source.py'))] if implementation == 'python' else [str(binary)]
      try:
        process = subprocess.run(arguments, input=json.dumps(config) + '\n', capture_output=True, text=True, timeout=10)
        packets = []
        while collector.poll(20):
          packets.append(collector.recv().hex())
      finally:
        collector.close(0)
        context.term()
      (output / f'{name}-{implementation}.stdout').write_text(process.stdout)
      (output / f'{name}-{implementation}.stderr').write_text(process.stderr)
      result = {'implementation': implementation, 'exit_code': process.returncode,
                'stdout': process.stdout, 'stderr': process.stderr, 'trace': lines(config['trace']), 'log_packets': packets}
      results.append(result)
  left, right = results
  assert left['trace'] == right['trace'], (name, left, right)
  assert left['stdout'] == right['stdout'], (name, left, right)
  assert not left['log_packets'] and not right['log_packets'], (name, results)
  if left['exit_code'] == -6:
    assert right['exit_code'] == 1 and ('Integer' in right['stderr'] or 'stoi' in right['stderr']), results
  else:
    assert left['exit_code'] == right['exit_code'], (name, results)
    assert bool(left['stderr']) == bool(right['stderr']), (name, results)
  (output / (name + '.comparison.json')).write_text(json.dumps(results, indent=2) + '\n')


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  all_alerts = [{'kind': 'alert', 'value': value} for value in [item for alert in [*range(37), 65535, 37, 0] for item in [alert, alert]]]
  cases = [
    ('every-alert', {'actions': all_alerts}, b'10'),
    ('every-alert-muted', {'actions': all_alerts}, b'5'),
    ('absent-volume', {'actions': [{'kind': 'alert', 'value': 3}]}, None),
    ('empty-volume', {'actions': [{'kind': 'alert', 'value': 3}]}, b''),
    ('prefix-volume', {'actions': [{'kind': 'alert', 'value': 1}]}, b' \v+6suffix'),
    ('mid-pulse-volume', {'actions': [{'kind': 'alert', 'value': 3}], 'mutations': {1: list(b'5'), 3: list(b'10')}}, b'10'),
    ('failed-status', {'actions': [{'kind': 'alert', 'value': 3}], 'status': 7}, b'10'),
    ('export-spawn-failure', {'actions': [{'kind': 'alert', 'value': 1}], 'command_fail': [0]}, b'10'),
    ('direction-spawn-failure', {'command_fail': [1]}, b'10'),
    ('startup-on-failure', {'command_fail': [2]}, b'10'),
    ('startup-off-failure', {'command_fail': [3]}, b'10'),
    ('worker-on-failure', {'actions': [{'kind': 'alert', 'value': 1}, {'kind': 'alert', 'value': 2}], 'command_fail': [4]}, b'10'),
    ('worker-off-failure', {'actions': [{'kind': 'alert', 'value': 1}, {'kind': 'alert', 'value': 2}], 'command_fail': [5]}, b'10'),
    ('startup-sleep-failure', {'sleep_fail': [0]}, b'10'),
    ('worker-sleep-failure', {'actions': [{'kind': 'alert', 'value': 1}, {'kind': 'alert', 'value': 2}], 'sleep_fail': [1]}, b'10'),
    ('invalid-startup-volume', {}, b'bad'),
    ('overflow-startup-volume', {}, b'2147483648'),
    ('invalid-worker-volume', {'actions': [{'kind': 'volume', 'bytes': list(b'bad')}, {'kind': 'alert', 'value': 1}]}, b'10'),
    ('invalid-off-pulse-volume', {'actions': [{'kind': 'alert', 'value': 3}], 'mutations': {1: list(b'bad')}}, b'10'),
    ('ratekeeper-catchup', {'mode': 'rate', 'times': [10, 10, 10, 10.001, 10.1, 10.14, 10.15, 10.16, 10.16, 10.16], 'count': 4}, b'10'),
  ]
  for name, settings, volume in cases:
    execute(args.binary.resolve(), args.binding.resolve(), args.output.resolve(), name, settings, volume)
  (args.output / 'summary.json').write_text(json.dumps({'passed': True, 'scenarios': len(cases), 'alerts': list(range(37)) + [65535, 37]}, indent=2) + '\n')
  print(f'PASS: {len(cases)} unchanged-source/native startup, alert, volume, worker-error and cadence scenarios')


if __name__ == '__main__':
  main()
