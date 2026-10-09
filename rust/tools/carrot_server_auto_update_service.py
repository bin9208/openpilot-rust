from __future__ import annotations

import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from carrot_server_auto_update_service_cases import CASES, Case
from carrot_server_auto_update_pull_cases import assert_repository, git, template

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'rust/tools'


def execute(case: Case, root: Path, base: Path, heads: dict[str, str], binary: Path | None, notify_url: str) -> dict:
  root.mkdir(parents=True)
  repository, state, params = root / 'repository', root / 'state', root / 'params'
  shutil.copytree(base, repository)
  assert_repository(repository)
  if case.dirty:
    (repository / 'tracked.txt').write_text('owned dirty checkout\n')
  if case.blocked:
    state.write_text('owned state path blocker\n')
  else:
    state.mkdir()
    initial = {'unrelated': 'preserved'}
    if case.initial:
      initial['auto_update'] = {key: heads['target'] if value == 'head' else value for key, value in case.initial.items()}
    (state / 'git.json').write_text(json.dumps(initial))
  (params / 'd').mkdir(parents=True)
  (params / 'd/DongleId').write_bytes(b' unknown ')
  (params / 'd/HardwareSerial').write_bytes('  owned-기기  '.encode())
  (params / 'd/Offroad_CarrotAutoUpdateFailed').write_text('prior-alert')
  (root / 'repository.lock').touch()
  config = {
    'mode': case.mode, 'repository': str(repository), 'source': str(ROOT), 'state': str(state), 'params': str(params),
    'lock': str(root / 'repository.lock'), 'launcher': str(ROOT / 'rust/target/debug/openpilot-process-child'),
    'head': '' if case.old == 'empty' else heads[case.old], 'reboot_mode': case.reboot_mode, 'steps': case.steps,
  }
  environment = os.environ | {
    'PYTHONPATH': os.environ.get('PYTHONPATH', '') + ':' + str(TOOLS) + ':' + str(ROOT), 'CWEB_PUSH_NOTIFY_URL': notify_url,
    'CWEB_PUSH_REPORT_TOKEN': '  owned-token  ', 'CARROT_REPO_LOCK_PATH': config['lock'],
  }
  command = [str(binary)] if binary else [sys.executable, '-P', str(TOOLS / 'carrot_server_auto_update_service_source.py')]
  start = time.monotonic()
  run = subprocess.run(command, input=json.dumps(config) + '\n', text=True, capture_output=True, env=environment, cwd=ROOT, timeout=15)
  row = {'argv': command, 'config': config, 'exit': run.returncode, 'seconds': time.monotonic() - start, 'stdout': run.stdout, 'stderr': run.stderr}
  (root / 'invocation.json').write_text(json.dumps(row, indent=2))
  assert run.returncode == 0, row
  result = json.loads(run.stdout.splitlines()[-1])
  return result


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--notify-url', default='http://127.0.0.1:1/notify')
  parser.add_argument('--case', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  environment = os.environ | {'GIT_AUTHOR_DATE': '2026-09-01T00:00:00+00:00', 'GIT_COMMITTER_DATE': '2026-09-01T00:00:00+00:00'}
  base, heads = template(args.output, environment)
  for index in range(12):
    (base / 'notification.txt').write_text(f'owned line {index}\n')
    git(base, 'add', 'notification.txt')
    subprocess.run(['/usr/bin/git', 'commit', '-qm', f'알림 {index} | subject'], cwd=base, env=environment, check=True)
  heads['target'] = git(base, 'rev-parse', 'HEAD')
  pairs = []
  for case in CASES:
    if args.case and case.name not in args.case:
      continue
    source = execute(case, args.output / case.name / 'source', base, heads, None, args.notify_url)
    native = execute(case, args.output / case.name / 'native', base, heads, args.binary, args.notify_url)
    row = {'name': case.name, 'case': asdict(case), 'source': source, 'native': native, 'equal': source == native}
    pairs.append(row)
    print(json.dumps({'name': case.name, 'equal': row['equal']}), flush=True)
  receipt = {'pairs': pairs, 'differences': [row['name'] for row in pairs if not row['equal']]}
  (args.output / 'result.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False))
  assert not receipt['differences'], receipt['differences']


if __name__ == '__main__':
  main()
