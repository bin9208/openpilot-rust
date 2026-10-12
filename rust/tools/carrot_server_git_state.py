from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from carrot_server_git_state_cases import cases


def snapshot(path: Path) -> dict:
  result = {}
  for name in ['git.json', 'git.json.tmp']:
    file = path / name
    if file.is_file():
      result[name] = dict(kind='file', bytes=file.read_bytes().hex(), mode=file.stat().st_mode & 0o777)
    elif file.is_dir():
      result[name] = dict(kind='directory')
    else:
      result[name] = None
  return result


def execute(command: list[str], case: dict, root: Path, traced: bool) -> dict:
  root.mkdir(parents=True)
  state = root / 'state'
  if case.get('state_file'):
    state.write_bytes(b'not-directory')
  elif not case.get('state_missing'):
    state.mkdir()
    if case.get('directory_file'):
      (state / 'git.json').mkdir()
    elif 'raw' in case:
      (state / 'git.json').write_bytes(case['raw'])
  old = (state / 'git.json').stat().st_ino if (state / 'git.json').is_file() else None
  before = snapshot(state) if state.is_dir() else {'state': 'file' if state.exists() else 'missing'}
  full_command = (['strace', '-yy', '-o', str(root / 'syscalls.log'), '-e', 'trace=openat,write,fsync,rename,renameat,renameat2'] + command) if traced else command
  process = subprocess.Popen(full_command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=(root / 'stderr.log').open('w'), text=True)
  config = dict(directory=str(state))
  process.stdin.write(json.dumps(config) + '\n')
  process.stdin.flush()
  observations = []
  for step in case['steps']:
    if step.get('fixture_action') == 'repair_target':
      (state / 'git.json').rmdir()
      observations.append(dict(input=step, output=None, files=snapshot(state)))
      continue
    process.stdin.write(json.dumps(step, ensure_ascii=True) + '\n')
    process.stdin.flush()
    line = process.stdout.readline()
    if not line:
      raise RuntimeError(f'Git state owned peer exited: {root}')
    observations.append(dict(input=step, output=json.loads(line), files=snapshot(state) if state.is_dir() else {'state': 'file' if state.exists() else 'missing'}))
  process.stdin.close()
  code = process.wait(timeout=5)
  if code != 0:
    raise RuntimeError(f'Git state owned peer failed: {code} {root}')
  current = (state / 'git.json').stat().st_ino if (state / 'git.json').is_file() else None
  receipt = dict(command=full_command, config=config, before=before, observations=observations, inode_before=old, inode_after=current, inode_replaced=old is not None and current is not None and old != current, exit_code=code)
  (root / 'receipt.json').write_text(json.dumps(receipt, ensure_ascii=True, indent=2) + '\n')
  if traced:
    trace = (root / 'syscalls.log').read_text()
    sync = trace.find('fsync(')
    rename = min((i for marker in ['rename(', 'renameat(', 'renameat2('] if (i := trace.find(marker)) >= 0), default=-1)
    if sync < 0 or rename < sync or 'git.json.tmp' not in trace[sync:rename]:
      raise RuntimeError(f'owned fsync/replace trace missing: {root}')
  return receipt


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  free = shutil.disk_usage(args.output.parent).free
  if free < 25 * 1024 ** 3 + 32 * 1024 ** 2:
    raise RuntimeError('Git state fixture requires 25GiB +32MiB free')
  args.output.mkdir(parents=True, exist_ok=False)
  (args.output / 'space-guard.json').write_text(json.dumps(dict(free_bytes=free, floor=25 * 1024 ** 3, growth=32 * 1024 ** 2)) + '\n')
  source = Path(__file__).with_name('carrot_server_git_state_source.py')
  sides = [('source', [sys.executable, '-P', str(source)])]
  if args.binary is not None:
    sides.append(('native', [str(args.binary.resolve())]))
  results = []
  for case in cases():
    outputs = {}
    for side, command in sides:
      outputs[side] = execute(command, case, args.output / case['name'] / side, case['name'] == 'compact_unicode')
    if args.binary is not None:
      compared = lambda r: (r['before'], r['observations'], r['inode_replaced'])
      if compared(outputs['source']) != compared(outputs['native']):
        (args.output / 'difference.json').write_text(json.dumps(dict(case=case['name'], outputs=outputs), ensure_ascii=True, indent=2) + '\n')
        raise RuntimeError(f'original/native Git state difference: {case["name"]}')
    results.append(dict(case=case['name'], artifacts={side: str(args.output / case['name'] / side / 'receipt.json') for side in outputs}))
    print('PASS', case['name'], flush=True)
  files = [Path(__file__), source, Path(__file__).with_name('carrot_server_git_state_cases.py')]
  if args.binary is not None:
    files.append(args.binary)
  (args.output / 'result.json').write_text(json.dumps(dict(passed=True, compared=args.binary is not None, cases=results, files={str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}), indent=2) + '\n')


if __name__ == '__main__':
  main()
