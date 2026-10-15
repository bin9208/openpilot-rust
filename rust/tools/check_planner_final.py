"""Compare final pinned helper bytes with native planner state transitions."""

import argparse
import json
import math
import random
import subprocess
import numpy as np
from pathlib import Path
from planner_source import module


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  args = parser.parse_args()
  coast_module = module('openpilot/selfdrive/controls/lib/cruise_coasting.py')
  lane_module = module('openpilot/selfdrive/controls/lib/lane_model_speed.py')
  gap_module = module('openpilot/selfdrive/controls/lib/longitudinal_gap_recovery.py')
  comfort_module = module('openpilot/selfdrive/controls/lib/longitudinal_braking_gap.py')
  comfort = comfort_module.LeadComfortState()
  coast = coast_module.CruiseCoastingPlan()
  lane = lane_module.LaneModelSpeedGuard(20)
  gap = gap_module.LeadGapState()
  actions, expected = [], []

  def add(op, **fields):
    actions.append(dict(op=op, **fields))
    if op == 'coasting':
      expected.append([coast.update(**fields['input']), coast.stable_time])
    if op == 'lane':
      expected.append(lane.update(fields['ego'], fields['start'], fields['end']))
    if op == 'comfort':
      comfort.update(**{k: fields[k] for k in ('enabled', 'track_id', 'dt', 'distance', 'relative_speed')})
      expected.append(
        comfort.comfort_relief(
          dt=fields['dt'],
          enabled=fields['enabled'],
          base_tf=fields['base_tf'],
          ego_speeds=np.array(fields['speeds']),
          lead_speeds=np.array(fields['leads']),
          lead_accel=fields['lead_accel'],
          physical_gaps=np.array(fields['gaps']),
          danger_margins=np.array(fields['danger']),
          times=np.array([0.0, 1.0, 3.0]),
          stop_distance=fields['stop'],
        ).tolist()
      )
    if op == 'forecast':
      expected.append(
        gap.margins(
          level=fields['level'],
          times=np.array([0.0, 1.0, 3.0]),
          ego_speeds=np.array(fields['speeds']),
          lead_speeds=np.array(fields['leads']),
          base_tf=fields['base_tf'],
          stop_distance=fields['stop'],
        ).tolist()
      )
    if op == 'gap':
      result = gap.update(**fields['input'])
      expected.append([result, dict(vars(gap), key=list(gap.key) if gap.key is not None else None)])

  rng = random.Random(338)
  for i in range(900):
    add(
      'coasting',
      input={
        'enabled': i % 73 != 0,
        'percent': (i // 90) % 11,
        'set_speed': 80 + i // 110,
        'target': 20 + math.sin(i / 25),
        'external_limit': 21 if i % 37 == 0 else 30,
        'dt': 0.05,
      },
    )
    add('lane', ego=20.0, start=10.0 if i % 89 == 0 else 20.0, end=13.0 if i % 71 == 0 else 20.0)
    add(
      'gap',
      input={
        'level': (i // 130) % 6,
        'track_id': i // 240,
        'enabled': i % 137 != 0,
        'dt': rng.choice([0.025, 0.05, 0.1]),
        'ego_speed': 20.0,
        'lead_speed': rng.choice([0.0, 0.3, 2.0, 22.0]),
        'relative_speed': 2.0 if i % 90 < 45 else -1.0,
        'distance': 35 + (i % 150) * 0.25,
        'desired_distance': 35.0,
        'base_tf': 1.45,
        'stop_distance': 4.0 if i % 200 < 100 else 9.0,
      },
    )
    add('forecast', level=(i // 130) % 6, speeds=[20.0, 19.0, 17.0], leads=[22.0, 21.0, 18.0], base_tf=1.45, stop=9.0)
    add(
      'comfort',
      enabled=i % 137 != 0,
      track_id=i // 240,
      dt=0.05,
      distance=60.0,
      relative_speed=-1.0,
      base_tf=1.45,
      speeds=[20.0, 19.0, 17.0],
      leads=[19.0, 18.0, 16.0],
      lead_accel=-1.0,
      gaps=[60.0, 58.0, 56.0],
      danger=[20.0, 19.0, 18.0] if i % 111 else [20.0, 3.0, 18.0],
      stop=6.0,
    )
  actual = json.loads(subprocess.check_output([str(args.binary.resolve())], input=json.dumps(actions).encode()))

  def equal(a, b, path=''):
    if isinstance(a, dict):
      assert a.keys() == b.keys(), (path, a, b)
      for k in a:
        equal(a[k], b[k], path + '.' + k)
    elif isinstance(a, list):
      assert len(a) == len(b), (path, a, b)
      for i, (x, y) in enumerate(zip(a, b, strict=False)):
        equal(x, y, f'{path}[{i}]')
    elif isinstance(a, float):
      assert math.isclose(a, b, rel_tol=2e-13, abs_tol=2e-13), (path, a, b)
    else:
      assert a == b, (path, a, b)

  equal(expected, actual)
  print(json.dumps({'source': 'e8e152dbcfd9574a1791ed12026d1880e87689b3', 'transitions': len(actions), 'status': 'passed'}))


if __name__ == '__main__':
  main()
