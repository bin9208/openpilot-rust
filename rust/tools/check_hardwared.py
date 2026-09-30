"""Focused source/native hardwared comparisons; no board access or daemon selection."""

import argparse
import json
import math
from pathlib import Path
import subprocess

from hardwared_source import run


def cases():
  for device in ['tici', 'tizi', 'mici']:
    yield (
      f'fan-{device}',
      {
        'kind': 'fan',
        'device': device,
        'steps': [(temp, ign) for ign in [False, True, False] for temp in [40.0, 60.0, 69.9, 75.0, 80.0, 95.0, 100.0, 110.0, 70.0] * 8],
      },
    )

  startup = dict.fromkeys(
    [
      'up_to_date',
      'no_excessive_actuation',
      'not_uninstalling',
      'accepted_terms',
      'free_space',
      'completed_training',
      'not_driver_view',
      'not_taking_snapshot',
      'registered_device',
    ],
    True,
  )

  def steps(changes):
    result = []
    for index, change in enumerate(changes):
      value = {
        'now': 1000.0 + index * 0.5,
        'frame': index * 5,
        'panda_updated': True,
        'panda_present': True,
        'panda_receive_time': 1000.0 + index * 0.5,
        'ignition': True,
        'in_car': True,
        'offroad_temperature': 60.0,
        'pmic_temperature': 60.0,
        'startup': dict(startup),
        'booted': True,
      }
      value.update(change)
      result.append(value)
    return result

  yield (
    'normal-cycle-disconnect',
    {
      'kind': 'policy',
      'device': 'tici',
      'steps': steps(
        [{}, {}, {}, {}, {'cycle_requested': True}, {}, {}, {}, {'ignition': False}, {}, {}, {'panda_updated': False, 'panda_receive_time': 990.0}]
      ),
    },
  )
  yield (
    'panda-disconnect-timeout',
    {
      'kind': 'policy',
      'device': 'tici',
      'steps': steps(
        [
          {},
          {},
          {},
          {'now': 1006.0, 'panda_updated': False, 'panda_receive_time': 1001.0},
          {'now': 1006.001, 'panda_updated': False, 'panda_receive_time': 1001.0},
        ]
      ),
    },
  )
  yield (
    'startup-block-and-boot-latch',
    {
      'kind': 'policy',
      'device': 'mici',
      'steps': steps(
        [
          {'booted': False},
          {'booted': False},
          {'startup': startup | {'accepted_terms': False}},
          {},
          {'booted': False},
          {'startup': startup | {'free_space': False}},
          {'ignition': False},
          {'startup': startup | {'not_driver_view': False}},
          {},
        ]
      ),
    },
  )
  yield (
    'thermal-hysteresis',
    {
      'kind': 'policy',
      'device': 'mici',
      'steps': steps(
        [{}] * 4 + [{'offroad_temperature': 100.0, 'pmic_temperature': 115.0}] * 70 + [{'offroad_temperature': 60.0, 'pmic_temperature': 60.0}] * 80
      ),
    },
  )
  yield (
    'offroad-danger-boundary',
    {
      'kind': 'policy',
      'device': 'tici',
      'steps': steps([{'offroad_temperature': 75.0, 'ignition': False}, {'offroad_temperature': 76.0, 'ignition': False}, {}, {}, {}]),
    },
  )
  yield 'tesla-awake-after-start', {'kind': 'policy', 'device': 'tizi', 'steps': steps([{'tesla': True}] * 4 + [{'tesla': True, 'ignition': False}] * 5)}
  yield (
    'ignition-edge-between-ticks',
    {'kind': 'policy', 'device': 'tici', 'steps': steps([{}, {}, {}, {'frame': 11, 'ignition': False}, {'frame': 12, 'ignition': False}, {'frame': 13}])},
  )
  shutdown = {
    'now': 4000.0,
    'ignition': False,
    'in_car': True,
    'off_ts': 0.0,
    'started_seen': True,
    'max_offroad_minutes': 1800,
    'disable': False,
    'force': False,
  }
  power_steps = []
  for now, voltage, ignition, power, changes in [
    (10.0, 12000.0, False, 10.0, {}),
    (20.0, 12000.0, False, 10.0, {}),
    (30.0, 14000.0, True, 10.0, {}),
    (40.0, None, False, 10.0, {}),
    (50.0, 11000.0, False, 10.0, {}),
    (60.0, 11000.0, False, -10.0, {}),
    (49.0, 11000.0, True, 10.0, {}),
    (70.0, 12000.0, False, 10.0, {'force': True, 'started_seen': False, 'now': 3600.0}),
    (80.0, 12000.0, False, 10.0, {'force': True, 'started_seen': False, 'now': 3600.001}),
    (90.0, 12000.0, False, 10.0, {'force': True, 'off_ts': None}),
    (100.0, 12000.0, False, 10.0, {'max_offroad_minutes': 1, 'now': 300.0}),
    (110.0, 12000.0, False, 10.0, {'max_offroad_minutes': 1, 'now': 300.001}),
    (120.0, 12000.0, False, 10.0, {'max_offroad_minutes': 1, 'disable': True}),
  ]:
    power_steps.append({'now': now, 'voltage': voltage, 'ignition': ignition, 'power': power, 'shutdown': shutdown | changes})
  yield 'power-integration-shutdown-boundaries', {'kind': 'power', 'capacity': 30e6, 'steps': power_steps}
  yield (
    'low-voltage-shutdown',
    {
      'kind': 'power',
      'capacity': 3e6,
      'steps': [{'now': 1000.0 + i, 'voltage': 10000.0, 'ignition': False, 'power': 100.0, 'shutdown': shutdown | {'now': 1000.0 + i}} for i in range(30)],
    },
  )


def compare(expected, actual, path=''):
  if isinstance(expected, dict):
    for key, value in expected.items():
      compare(value, actual[key], f'{path}.{key}')
  elif isinstance(expected, list):
    assert len(expected) == len(actual), (path, len(expected), len(actual))
    for index, (left, right) in enumerate(zip(expected, actual, strict=True)):
      compare(left, right, f'{path}[{index}]')
  elif isinstance(expected, float):
    assert math.isclose(expected, actual, rel_tol=1e-12, abs_tol=1e-8), (path, expected, actual)
  else:
    assert expected == actual, (path, expected, actual)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  tests = list(cases())
  result = subprocess.run(
    [*args.runner, str(args.binary.resolve())], input=''.join(json.dumps(case) + '\n' for _, case in tests), capture_output=True, text=True, check=True
  )
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr or '<empty>\n')
  native = [json.loads(line) for line in result.stdout.splitlines()]
  reference = []
  for (name, case), actual in zip(tests, native, strict=True):
    expected = run(case)
    reference.append({'name': name, 'input': case, 'expected': expected})
    (args.output / 'source.json').write_text(json.dumps(reference, indent=2))
    compare(expected, actual, name)
    print(f'PASS {name}: {len(expected)} observations')
  (args.output / 'result.json').write_text(
    json.dumps({'passed': len(tests), 'source': 'unchanged hardwared.py/fan_controller.py/power_monitoring.py', 'device_access': False}, indent=2) + '\n'
  )


if __name__ == '__main__':
  main()
