#!/usr/bin/env python3
"""Focused LSM6DS3 driver/loop/wire comparison against unchanged source bodies."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def cases():
  return {
    'normal_69': {'chip': 0x69},
    'self_test_6a': {'chip': 0x6A, 'self_test': '1'},
    'gyro_only_env': {'chip': 0x6A, 'self_test': '0'},
    'self_test_failure': {'self_test': '1', 'fail_self_test': True},
    'chip_failure': {'chip': 0x68},
    'irq': {
      'mode': 'irq',
      'frames': [
        {'mono': 1.0},
        {'poll': 'timeout'},
        {'poll': 'other'},
        {'mono': 1.6, 'offset': 1010000000},
        {'mono': 1.7, 'offset': 1010000001},
        {'mono': 1.8, 'offset': 1010000001, 'fault': 0x28},
        {'mono': 1.9, 'offset': 1010000001, 'ready': 0},
        {'mono': 2.0, 'offset': 1010000001},
        {'poll': 'short'},
      ],
    },
    'poll': {
      'mode': 'poll',
      'frames': [
        {'mono': 1.0},
        {'mono': 1.5},
        {'mono': 1.500001},
        {'mono': 1.9, 'fault': 0x20},
        {'mono': 2.4},
      ],
    },
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  report = []
  for case, request in cases().items():
    results = []
    for name, command in [('source', [sys.executable, str(Path(__file__).with_name('sensord_source.py'))]), ('native', [str(args.trace.resolve())])]:
      result = subprocess.run(
        command,
        input=json.dumps(request),
        text=True,
        capture_output=True,
        timeout=15,
        env=os.environ | {'PYTHONPATH': str(Path(__file__).resolve().parents[2])},
      )
      assert result.returncode == 0, result.stderr
      output = json.loads(result.stdout)
      (args.evidence / f'{case}-{name}.json').write_text(json.dumps(output, indent=2) + '\n')
      (args.evidence / f'{case}-{name}.stderr').write_text(result.stderr or '(no stderr)\n')
      results.append(output)
    assert results[0] == results[1], f'mismatch: {case}'
    if case == 'irq':
      assert len(results[1]['packets']) == 5
      assert ['warning', 'time jumped: 1010000001 1000000000'] in results[1]['logs']
      assert ['exception', 'Error processing accelerometer'] in results[1]['logs']
      assert results[1]['rows'][-4] == {'error': 'short GPIO event'}
    if case == 'poll':
      assert len(results[1]['packets']) == 2
      assert ['exception', 'Error in temperatureSensor polling loop'] in results[1]['logs']
    report.append({'scenario': case, 'pass': True, 'artifact': f'{case}-native.json'})
  (args.evidence / 'policy-results.json').write_text(json.dumps(report, indent=2) + '\n')
  print(json.dumps({'passed': len(report), 'evidence': str(args.evidence)}))


if __name__ == '__main__':
  main()
