"""Bounded read-only evidence for an already failed owned Card shutdown.

The worker has only stdlib imports and never signals, attaches to, or waits for
the peer. Its caller retains the original shutdown TimeoutExpired.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

FILE_BYTES = 4096
TOTAL_BYTES = 128 * 1024
MAX_TASKS = 32
PROC_SECONDS = .5
WORKER_SECONDS = 1.
JSON_BYTES = 1024 * 1024


def write_json(path, value):
  data = (json.dumps(value, indent=2) + '\n').encode()
  if len(data) > JSON_BYTES:
    raise ValueError('attribution receipt exceeds one MiB')
  pending = path.with_name(path.name + '.pending')
  pending.write_bytes(data)
  pending.replace(path)


def process_snapshot(pid, parent_pid, *, proc_root=Path('/proc')):
  if type(pid) is not int or pid <= 0 or type(parent_pid) is not int or parent_pid <= 0:
    raise ValueError('attribution requires positive owned PID/parent PID')
  started = time.monotonic()
  deadline = started + PROC_SECONDS
  remaining = TOTAL_BYTES
  result = {'pid': pid, 'identity': {'status': 'unavailable'}, 'process': {}, 'threads': [], 'threads_truncated': False,
    'limits': {'file_bytes': FILE_BYTES, 'total_bytes': TOTAL_BYTES, 'tasks': MAX_TASKS, 'worker_seconds': WORKER_SECONDS,
               'proc_seconds': PROC_SECONDS},
    'python_stack': {'status': 'unavailable', 'reason': 'no established peer faulthandler registration; no signal sent'},
    'userspace_stack': {'status': 'unavailable', 'reason': 'no ptrace attachment; available kernel stacks are recorded per task'}}

  def read(path):
    nonlocal remaining
    if remaining <= 0 or time.monotonic() >= deadline:
      return {'status': 'unavailable', 'reason': 'capture budget exhausted'}
    limit = min(FILE_BYTES, remaining)
    try:
      fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK | os.O_NOFOLLOW)
      try:
        data = os.read(fd, limit + 1)
      finally:
        os.close(fd)
    except OSError as error:
      return {'status': 'unavailable', 'errno': error.errno, 'reason': str(error)[:240]}
    remaining -= min(limit, len(data))
    return {'status': 'ok', 'text': data[:limit].decode(errors='replace'), 'truncated': len(data) > limit}

  process = proc_root / str(pid)
  stat = result['process']['stat'] = read(process / 'stat')
  if stat['status'] == 'ok' and not stat['truncated']:
    try:
      before, separator, after = stat['text'].rpartition(')')
      fields = after.split()
      if not separator or int(before.split('(', 1)[0]) != pid:
        raise ValueError('malformed process identity')
      actual_parent, start = int(fields[1]), int(fields[19])
      result['identity'] = {'status': 'owned_child' if actual_parent == parent_pid else 'not_owned_child',
                            'parent_pid': actual_parent, 'start_time_ticks': start}
    except (ValueError, IndexError) as error:
      result['identity'] = {'status': 'unavailable', 'reason': str(error)[:240]}
  if result['identity']['status'] == 'owned_child':
    for name in ('wchan', 'schedstat', 'stack'):
      result['process'][name] = read(process / name)
    try:
      with os.scandir(process / 'task') as entries:
        for index, entry in enumerate(entries):
          if index >= MAX_TASKS or remaining <= 0 or time.monotonic() >= deadline:
            result['threads_truncated'] = True
            break
          if not entry.name.isascii() or not entry.name.isdecimal():
            continue
          result['threads'].append({'tid': int(entry.name), 'files': {
            name: read(process / 'task' / entry.name / name) for name in ('stat', 'wchan', 'schedstat', 'stack')}})
    except OSError as error:
      result['thread_error'] = {'errno': error.errno, 'reason': str(error)[:240]}
  result['stored_bytes'] = TOTAL_BYTES - remaining
  result['elapsed_seconds'] = time.monotonic() - started
  return result


def publication_summary(row):
  result = {key: row[key] for key in ('logMonoTime', 'valid') if key in row}
  state = row.get('carState')
  if state is not None:
    result['canErrorCounter'] = state.get('canErrorCounter')
    radar = state.get('radarInput', {})
    result['radarInput'] = {key: radar.get(key) for key in ('firstCanMonoTime', 'lastCanMonoTime', 'canPacketCount', 'receiveMonoTime')}
  return result


def record_shutdown_timeout(peer, source, error, last_can, empty, captured):
  """Save partial lifecycle evidence before a separately time-bounded proc read."""
  receipt = {'event': 'runtime_shutdown_timeout', 'pid': peer.process.pid, 'source': source,
    'timeout_seconds': error.timeout, 'exception': type(error).__name__, 'observed_frames': peer.observed_frames,
    'existing_logs': ['stdout.log', 'stderr.log', 'frequency.jsonl'],
    'shutdown_signals_sent': ['SIGCONT', 'SIGINT'], 'last_nonempty_can': publication_summary(last_can),
    'first_empty_can': publication_summary(empty),
    'partial_publications': {name: {'count': len(rows), 'tail': [publication_summary(row) for row in rows[-4:]]}
                             for name, rows in captured.items()},
    'process_capture': {'status': 'pending', 'file': 'shutdown-process.json', 'timeout_seconds': WORKER_SECONDS}}
  path = peer.output / 'shutdown-timeout.json'
  write_json(path, receipt)
  # A stuck procfs read cannot hold the already-failed harness indefinitely.
  # subprocess.run may terminate only its own diagnostic worker on timeout.
  command = [sys.executable, str(Path(__file__).resolve()), '--pid', str(peer.process.pid), '--parent', str(os.getpid()),
             '--output', str((peer.output / 'shutdown-process.json').resolve())]
  try:
    result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                            timeout=WORKER_SECONDS, check=False)
    receipt['process_capture'].update(status='complete' if result.returncode == 0 else 'worker_failed', returncode=result.returncode)
  except subprocess.TimeoutExpired:
    receipt['process_capture']['status'] = 'worker_timeout'
  except OSError as failure:
    receipt['process_capture'].update(status='unavailable', reason=str(failure)[:240])
  write_json(path, receipt)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--pid', type=int, required=True)
  parser.add_argument('--parent', type=int, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  write_json(args.output, process_snapshot(args.pid, args.parent))


if __name__ == '__main__':
  main()
