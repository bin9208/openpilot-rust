#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# ─── How to run ───
# uv run rust/tools/check_selfdrived_ratekeeper.py --binary <ratekeeper> --output <evidence>
# ──────────────────
"""Compare the original timing class and every retained moving-average value."""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
from pathlib import Path
import random
import subprocess
from types import SimpleNamespace


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True, type=Path)
  parser.add_argument('--output', required=True, type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  root = Path(__file__).resolve().parents[2]
  timing = root / 'openpilot/common/realtime.py'
  average = root / 'openpilot/common/utils.py'
  rate_node = next(node for node in ast.parse(timing.read_text()).body if isinstance(node, ast.ClassDef) and node.name == 'Ratekeeper')
  avg_node = next(node for node in ast.parse(average.read_text()).body if isinstance(node, ast.ClassDef) and node.name == 'MovingAverage')
  ticks = iter(())
  namespace = {'time': SimpleNamespace(monotonic=lambda: next(ticks)), 'getproctitle': lambda: 'fixture'}
  exec(compile(ast.Module(body=[avg_node, rate_node], type_ignores=[]), str(timing), 'exec'), namespace)
  source = namespace['Ratekeeper'](100, print_delay_threshold=None)
  rng = random.Random(168)
  now = 10.
  rows, expected = [], []
  for index in range(20000):
    dt = (.01, .02, .001, .01 / .9, 0.)[index % 5] if index < 2000 else rng.uniform(.001, .03)
    now += dt
    values = [now] * (4 if index == 0 else 2)
    rows.append(values)
    ticks = iter(values)
    before = source.lagging
    lagged = source.monitor_time()
    expected.append({'rate': {'frame': source.frame, 'remaining': source.remaining,
                   'last_monitor_time': source._last_monitor_time, 'next_frame_time': source._next_frame_time,
                   **{key: value.copy() if isinstance(value, list) else value
                      for key, value in vars(source.avg_dt).items() if key != 'window_size'}},
                   'lagged': lagged, 'lagging_before': before, 'lagging_after': source.lagging})
  payload = ''.join(json.dumps(row)+'\n' for row in rows)
  (args.output / 'input.jsonl').write_text(payload)
  (args.output / 'source.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in expected))
  result = subprocess.run([str(args.binary.resolve())], input=payload, capture_output=True, text=True, check=True)
  (args.output / 'native.jsonl').write_text(result.stdout)
  (args.output / 'native.stderr').write_text(result.stderr or 'no native stderr\n')
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  assert actual == expected
  report = {'result': 'PASS', 'steps': len(rows), 'source_sha256': hashlib.sha256(timing.read_bytes()).hexdigest(),
            'moving_average_sha256': hashlib.sha256(average.read_bytes()).hexdigest(),
            'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
  (args.output / 'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
  print(json.dumps(report))


if __name__ == '__main__':
  main()
