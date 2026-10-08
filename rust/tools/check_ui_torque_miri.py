#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
# ─── How to run ───
# 1. Install uv (if not installed):
#      curl -LsSf https://astral.sh/uv/install.sh | sh
# 2. Run with repository native msgq modules on PYTHONPATH:
#      uv run check_ui_torque_miri.py [ARGS]
# 3. Or use the existing startup-ui Python environment without installing dependencies.
# ──────────────────

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from ui_application_qa.torque_geometry_source import render


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--target-dir', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  free = shutil.disk_usage(root).free
  assert free > (25 + 0.5) * 1024**3, free
  args.output.mkdir(parents=True, exist_ok=True)
  arc = {'cx': 268.5, 'cy': 1438.5, 'radius': 1207.5, 'thickness': 14.5, 'start': -96.05, 'end': -83.95}
  steps = [
    {'reset': True, 'arc': arc},
    {'arc': dict(arc, cx=268.501, start=-96.04999)},
    {'arc': dict(arc, start=-83.95, end=-96.05)},
    {'arc': dict(arc, start=-90, end=-90)},
    {'arc': dict(arc, thickness=0)},
    {'arc': dict(arc, thickness=1)},
    {'reset': True, 'arc': arc},
    {'arc': arc},
  ]
  (args.output / 'input.json').write_text(json.dumps(steps))
  expected = render(steps)
  (args.output / 'source.json').write_text(json.dumps(expected))
  command = [
    str(Path.home() / '.cargo/bin/cargo'),
    '+nightly-2026-09-29',
    'miri',
    'run',
    '--locked',
    '-j2',
    '-p',
    'openpilot-ui-application',
    '--example',
    'torque_geometry',
    '--',
    '--json',
    json.dumps(steps),
  ]
  environment = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(args.target_dir.resolve()))
  with (args.output / 'miri.log').open('w') as log:
    log.write('Invocation: ' + json.dumps(command) + '\n')
    log.flush()
    result = subprocess.run(command, cwd=root / 'rust', env=environment, stdout=subprocess.PIPE, stderr=log, text=True, check=True, timeout=600)
  (args.output / 'miri.json').write_text(result.stdout)
  actual = json.loads(result.stdout)
  assert actual == expected
  binary = args.target_dir / 'miri/x86_64-unknown-linux-gnu/debug/examples/torque_geometry'
  (args.output / 'result.json').write_text(
    json.dumps(
      {
        'verdict': 'PASS',
        'cases': len(steps),
        'exact_f32_points': True,
        'scope': 'bounded arc/cache-hit/reversed/equal/thin/zero-thickness/reset cases; native full corpus separately checks LRU eviction',
        'miri_runner_binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'command': command,
      },
      indent=2,
    )
  )
  print('PASS bounded native torque geometry under default Miri with exact original f32 points')


if __name__ == '__main__':
  main()
