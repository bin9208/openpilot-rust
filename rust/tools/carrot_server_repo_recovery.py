from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import TypedDict

from carrot_server_repo_recovery_cases import CASES, ROOT, Case, IndexState, index_state, protected, setup
from carrot_server_repo_recovery_source import Config, Result


class Receipt(TypedDict):
  argv: list[str]
  input: Config
  stdout: str
  stderr: str
  exit: int
  seconds: float
  payload: Result
  index: IndexState
  messages: list[str]
  git_argv: list[str]
  protected: dict[str, str]
  captured_child_reaped: bool


def execute(directory: Path, case: Case, binary: Path | None) -> Receipt:
  config = setup(directory, case)
  config.update(python=sys.executable, source=str(ROOT / 'rust/tools/carrot_server_repo_recovery_source.py'))
  before = protected(directory)
  env = {**os.environ, 'PATH': str(directory / 'bin'), 'GIT_CONFIG_GLOBAL': os.devnull,
         'GIT_CONFIG_NOSYSTEM': '1', 'CARROT_REPO_LOCK_PATH': config['lock'],
         'OWNED_RECOVERY_REPO': config['repo'], 'OWNED_RECOVERY_GIT': case.git,
         'OWNED_RECOVERY_TRACE': str(directory / 'trace.json'), 'PYTHONPATH': f'{ROOT / "rust/tools"}:{ROOT}'}
  command = [str(binary)] if binary else [sys.executable, '-P', config['source']]
  git_process = None
  if case.process == 'real':
    git_process = subprocess.Popen(['/usr/bin/git', 'hash-object', '--stdin'], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    deadline = time.monotonic() + 2
    while Path(f'/proc/{git_process.pid}/comm').read_text().strip() != 'git':
      assert time.monotonic() < deadline, 'real owned Git comm not ready'
      time.sleep(.005)
    (directory / 'real-git.json').write_text(json.dumps({'pid': git_process.pid, 'comm': 'git'}))
  start = time.monotonic()
  try:
    process = subprocess.run(command, input=json.dumps(config), text=True, capture_output=True,
                             env=env, timeout=15, check=True)
  finally:
    for path in (directory / 'proc', directory / 'proc/42/comm'):
      if path.exists():
        path.chmod(0o755 if path.is_dir() else 0o644)
    if git_process is not None:
      assert git_process.stdin is not None
      git_process.stdin.close()
      git_process.wait(timeout=2)
      assert git_process.returncode == 0
      assert git_process.stdout is not None and git_process.stderr is not None
      git_process.stdout.close()
      git_process.stderr.close()
  lines = process.stdout.splitlines()
  payload = json.loads(lines[-1])
  trace = json.loads((directory / 'trace.json').read_text())
  assert trace['inherited'] and trace['blocked']
  assert all(payload['locks'])
  assert before == protected(directory), 'Unrelated lock/config changed'
  try:
    os.kill(trace['pid'], 0)
  except ProcessLookupError:
    reaped = True
  else:
    raise AssertionError('captured Git child was not reaped')
  row: Receipt = {'argv': command, 'input': config, 'stdout': process.stdout, 'stderr': process.stderr,
         'exit': process.returncode, 'seconds': time.monotonic() - start, 'payload': payload,
         'index': index_state(directory), 'messages': lines[:-1], 'git_argv': trace['argv'],
         'protected': before, 'captured_child_reaped': reaped}
  (directory / 'receipt.json').write_text(json.dumps(row, indent=2))
  return row


def expected(case: Case, row: Receipt) -> None:
  output = row['payload']['output']
  if case.cancel:
    assert output['exception'] == 'CancelledError' and row['index']['kind'] == 'missing'
    assert len(row['payload']['locks']) == 3
  elif case.git == 'timeout':
    assert output['exception'] == 'TimeoutExpired' and 10 <= row['seconds'] < 12
  elif case.git in ('stdout_utf8', 'stderr_utf8'):
    assert output['exception'] == 'UnicodeDecodeError' and row['index']['kind'] == 'file'
  elif case.git in ('stderr', 'empty_error'):
    assert output['exception'] == 'RuntimeError'
  elif (case.index not in ('regular', 'missing') or case.age < 60
        or case.process in ('git', 'git-fetch', 'missing', 'denied', 'comm_denied', 'real')
        or case.action in ('replace', 'size', 'mtime', 'git_start', 'proc_remove')):
    assert output['exception'] == 'RepoBusyError'
  else:
    assert output['result'] == (case.index == 'regular' and case.action != 'remove')
    assert row['index']['kind'] == 'missing'


def main() -> None:
  binary = None if sys.argv[1] == '--source-only' else Path(sys.argv[1]).resolve()
  output = Path(sys.argv[2]).resolve()
  output.mkdir(parents=True, exist_ok=False)
  selected = set(sys.argv[3:])
  results = []
  for case in CASES:
    if selected and case.name not in selected:
      continue
    rows = []
    for side in ('source', 'native') if binary else ('source',):
      directory = output / case.name / side
      row = execute(directory, case, binary if side == 'native' else None)
      expected(case, row)
      rows.append(row)
    if binary:
      normalized = [json.loads(json.dumps(row).replace(str(output / case.name / side), '<SIDE>'))
                    for row, side in zip(rows, ('source', 'native'), strict=True)]
      for key in ('payload', 'index', 'messages', 'git_argv', 'protected'):
        assert normalized[0][key] == normalized[1][key], (case.name, key, normalized)
    results.append({'case': case.name, 'paired': binary is not None, 'seconds': [row['seconds'] for row in rows]})
    (output / 'result.json').write_text(json.dumps({'cases': results, 'complete': False}, indent=2))
    print(json.dumps(results[-1]), flush=True)
  assert results
  (output / 'result.json').write_text(json.dumps({'cases': results, 'complete': True}, indent=2))


if __name__ == '__main__':
  main()
