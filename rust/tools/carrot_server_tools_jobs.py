#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Retained small original/native Tools history sequence: reload, prune, Unicode logs and terminal fields."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import select
import subprocess
from types import SimpleNamespace

from carrot_server_dashcam_upload import save


def main() -> None:
  parser = argparse.ArgumentParser()
  for name in ['binary', 'output']:
    parser.add_argument('--' + name, type=Path, required=True)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True)
  paths = [output / side / 'tool_jobs.json' for side in ['source', 'native']]
  now = 1_000_000.0
  raw = [None, [], {'id': '', 'action': 'ignore'}, {'id': 'old', 'action': 'old', 'status': 'done', 'updated_at': 1}]
  raw.extend({'id': f'finished-{index}', 'action': 'notice', 'status': 'done', 'updated_at': 999_990.0, 'created_at': index} for index in range(22))
  raw.extend(
    [
      {'id': 'interrupted', 'action': 'shell_cmd', 'status': 'running', 'log': '😀' * 60_002, 'updated_at': now, 'created_at': now},
      {'id': 'padded', 'action': 'shell_cmd', 'status': ' running ', 'log': 0, 'message': None, 'updated_at': now, 'created_at': now},
      {'id': 'failure', 'action': 'shell_cmd', 'status': 'failed', 'payload': [], 'result': [], 'progress': 17, 'updated_at': now, 'created_at': now},
    ]
  )
  for path in paths:
    path.parent.mkdir()
    save(path, {'version': 1, 'jobs': raw})
  source = Path('openpilot/selfdrive/carrot/server/features/tools/jobs.py').resolve()
  spec = importlib.util.spec_from_file_location('original_tools_jobs', source)
  assert spec and spec.loader
  jobs = importlib.util.module_from_spec(spec)
  jobs.__package__ = 'openpilot.selfdrive.carrot.server.features.tools'
  spec.loader.exec_module(jobs)
  jobs.CARROT_STATE_DIR = str(paths[0].parent)
  jobs.CARROT_TOOL_JOBS_STATE_PATH = str(paths[0])
  jobs.time = SimpleNamespace(time=lambda: now)
  argv = [str(args.binary.resolve())]
  process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  rows = []
  assert process.stdin and process.stdout
  try:
    config = {'path': str(paths[1]), 'now': now}
    process.stdin.write(json.dumps(config) + '\n')
    process.stdin.flush()

    def pair(label: str, step: dict, expected) -> None:
      process.stdin.write(json.dumps(step, ensure_ascii=True) + '\n')
      process.stdin.flush()
      assert select.select([process.stdout], [], [], 5)[0], label
      actual = json.loads(process.stdout.readline())
      same = actual == expected
      row = {'scenario': label, 'input': step, 'equal': same, 'source': expected, 'native': actual}
      if not same:
        save(output / 'failure.json', row)
      assert same, label
      canonical = json.dumps(expected, ensure_ascii=True, sort_keys=True)
      rows.append({'scenario': label, 'equal': True, 'canonical_bytes': len(canonical), 'sha256': hashlib.sha256(canonical.encode()).hexdigest()})

    pair('reload-interruption-cap-age-and-stable-order', {'operation': 'list'}, jobs.list_snapshots())
    interrupted = jobs.jobs()['interrupted']
    assert interrupted['status'] == 'failed' and interrupted['error'] == 'server restarted before job completed' and len(interrupted['log']) == 60_000
    pair('interrupted-terminal-fields', {'operation': 'get', 'id': 'interrupted'}, jobs.snapshot(interrupted))
    for label, text in [('append-none', None), ('append-false', False), ('append-lines', 'x\r\ny\rz')]:
      jobs.append(jobs.jobs()['failure'], text)
      pair(label, {'operation': 'append', 'id': 'failure', 'text': text}, jobs.list_snapshots())
    result = {'ok': False, 'out': 'owned failure', 'error_code': 'OWNED', 'error_detail': 'detail'}
    jobs.finish(jobs.jobs()['failure'], ok=False, result=result)
    pair('failed-result-error-progress-unmasked', {'operation': 'finish', 'id': 'failure', 'ok': False, 'result': result}, jobs.list_snapshots())
    jobs.persist_now()
    pair('persist', {'operation': 'persist'}, None)
    source_state, native_state = [json.loads(path.read_text()) for path in paths]
    assert source_state == native_state
    save(output / 'persisted-comparison.json', {'equal': True, 'source': source_state, 'native': native_state})
    removed = jobs.clear_finished()
    pair('clear-finished', {'operation': 'clear'}, removed)
    process.stdin.close()
    process.wait(timeout=3)
    assert process.returncode == 0
  finally:
    if process.poll() is None:
      process.kill()
    process.wait(timeout=3)
    if process.stdout:
      process.stdout.close()
    if process.stderr:
      stderr = process.stderr.read()
      process.stderr.close()
      save(output / 'process.json', {'argv': argv, 'exit': process.returncode, 'stderr': stderr})
  save(output / 'result.json', {'cases': rows, 'actual_original_jobs': True, 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'pass': True})


if __name__ == '__main__':
  main()
