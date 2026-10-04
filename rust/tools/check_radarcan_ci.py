from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

from card_qa.ci import ROOT, TOOLS, hashes, require_space, run


def target_arguments(args: argparse.Namespace) -> list[str]:
  if args.runner is None:
    return []
  return ['--runner', str(args.runner), '--sysroot', str(args.sysroot)]


def reference(args: argparse.Namespace) -> None:
  for operation in ('common', 'base', 'decoder', 'hyundai', 'cluster', 'runtime'):
    require_space(args.evidence, 1024**3)
    command = [sys.executable, '-P', str(TOOLS / 'check_radarcan_policy.py'), '--binary', str(args.probe),
      '--binding', str(args.binding), '--dbc', str(args.dbc), '--numerics', str(args.numerics),
      '--evidence', str(args.evidence / operation)]
    if operation != 'common':
      command.extend(['--operation', operation])
    command.extend(target_arguments(args))
    run(command, args.evidence / 'commands', operation)
  require_space(args.evidence, 1024**3)
  run([sys.executable, '-P', str(TOOLS / 'check_radarcan_constructors.py'), '--binary', str(args.probe),
    '--binding', str(args.binding), '--dbc', str(args.dbc), '--evidence', str(args.evidence / 'constructors'),
    *target_arguments(args)],
    args.evidence / 'commands', 'constructors')


def ipc(args: argparse.Namespace) -> None:
  require_space(args.evidence, 64 * 1024**2)
  fixtures = args.evidence / 'fixtures'
  run([sys.executable, '-P', str(TOOLS / 'stage_radarcan_runtime.py'), '--dbc', str(args.dbc),
    '--binding', str(args.binding), '--output', str(fixtures)], args.evidence / 'commands', 'stage-runtime')
  for name in ('normal', 'joined', 'lifecycle'):
    require_space(args.evidence, 512 * 1024**2)
    command = [sys.executable, '-P', str(TOOLS / 'check_radarcan_runtime.py'), '--binary', str(args.daemon),
      '--binding', str(args.binding), '--dbc', str(args.dbc), '--numerics', str(args.numerics),
      '--cases', str(fixtures / ('joined.json' if name == 'joined' else 'normal.json')),
      '--evidence', str(args.evidence / name)]
    if name == 'lifecycle':
      command.append('--lifecycle')
    run(command, args.evidence / 'commands', name)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--phase', choices=('reference', 'ipc'), required=True)
  for option in ('probe', 'daemon', 'binding', 'dbc', 'numerics', 'evidence'):
    parser.add_argument('--' + option, type=Path, required=True)
  parser.add_argument('--runner', type=Path)
  parser.add_argument('--sysroot', type=Path)
  args = parser.parse_args()
  if (args.runner is None) != (args.sysroot is None):
    parser.error('--runner and --sysroot must be specified together')
  if args.runner and args.phase == 'ipc':
    parser.error('the IPC phase requires an actual host daemon; use reference for QEMU')
  for option in ('probe', 'daemon', 'binding', 'dbc', 'numerics', 'evidence'):
    setattr(args, option, getattr(args, option).resolve())
  require_space(args.evidence, 1024**3)
  args.evidence.mkdir(parents=True)
  sources = [*TOOLS.glob('*radarcan*.py'), *TOOLS.glob('tests/test_radarcan*.py'),
    *(ROOT / 'rust/crates/radarcan').rglob('*.rs')]
  (args.evidence / 'invocation.json').write_text(json.dumps({'argv': sys.argv, 'sources': hashes(sources),
    'binaries': {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (args.probe, args.daemon)}}, indent=2))
  if args.phase == 'reference':
    reference(args)
  else:
    ipc(args)
  (args.evidence / 'result.json').write_text(json.dumps({'status': 'pass', 'phase': args.phase}) + '\n')


if __name__ == '__main__':
  main()
