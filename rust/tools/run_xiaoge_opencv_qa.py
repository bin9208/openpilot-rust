#!/usr/bin/env python3
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Run --evidence DIR --phase NAME --cwd DIR --growth-mib N [--env KEY=VALUE] -- COMMAND...
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

class InsufficientSpace(OSError):
  def __init__(self, free: int, growth: int) -> None:
    self.free = free
    self.growth = growth
    super().__init__(f'require25GiB+growth={growth}; free={free}; recover35GiB before resuming')


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--phase', required=True)
  parser.add_argument('--cwd', type=Path, required=True)
  parser.add_argument('--growth-mib', type=int, required=True)
  parser.add_argument('--env', action='append', default=[])
  parser.add_argument('command', nargs=argparse.REMAINDER)
  args = parser.parse_args()
  if args.growth_mib < 0 or not args.command:
    parser.error('growth must be nonnegative and command required')
  command = args.command[1:] if args.command[0] == '--' else args.command
  overrides = dict(value.split('=', 1) for value in args.env)
  free = shutil.disk_usage(args.cwd).free
  growth = args.growth_mib * 1024**2
  args.evidence.mkdir(parents=True, exist_ok=True)
  record = {'argv': command, 'cwd': str(args.cwd), 'env': overrides, 'free_bytes': free,
    'growth_bytes': growth, 'required_bytes': 25 * 1024**3 + growth}
  (args.evidence / (args.phase + '.command.json')).write_text(json.dumps(record, indent=2) + '\n')
  if free < 25 * 1024**3 + growth:
    raise InsufficientSpace(free, growth)
  started = time.monotonic()
  with (args.evidence / (args.phase + '.log')).open('w') as log:
    try:
      result = subprocess.run(command, cwd=args.cwd, env=dict(os.environ, **overrides), stdout=log, stderr=log, check=False)
      returncode = result.returncode
    except FileNotFoundError as error:
      log.write(f'launch failed: {error}\n')
      returncode = 127
  status = {'returncode': returncode, 'seconds': time.monotonic() - started}
  (args.evidence / (args.phase + '.exit.json')).write_text(json.dumps(status) + '\n')
  print(json.dumps(status), flush=True)
  raise SystemExit(returncode)


if __name__ == '__main__':
  main()
