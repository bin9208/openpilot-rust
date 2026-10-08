import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from web_upload_eintr_peer import Peer, blocked, interrupt, launch, stop_child, until

ROOT = Path(__file__).resolve().parents[2]


def run(side, binary, root, late):
  peer = Peer()
  request = dict(op='session', base=peer.base, metadata={'dongle_id': 'owned'}, purpose='tmux', sync=True)
  command = [str(binary.resolve())] if side == 'native' else [sys.executable, '-P', str(Path(__file__).with_name('web_upload_eintr_source.py'))]
  env = dict(os.environ, NO_PROXY='127.0.0.1,localhost', PYTHONPATH=str(ROOT) + ':' + os.environ.get('PYTHONPATH', ''))
  process, pid, trace, invocation, identity = launch(command, root, request, env)
  receipts = []
  try:
    until(peer.received.is_set)
    initial_wait = until(lambda: blocked(pid))
    started = time.monotonic()
    interrupt(pid, trace, receipts)
    if side == 'native':
      until(lambda: 'EINTR' in trace.read_text())
    if late:
      until(lambda: time.monotonic() >= started + 6 or process.poll() is not None, timeout=7)
      if process.poll() is None:
        interrupt(pid, trace, receipts)
      until(lambda: process.poll() is not None, timeout=9)
    else:
      peer.release.set()
    stdout, _ = process.communicate(timeout=15)
    ended = time.monotonic()
    peer.close()
    (root / 'stdout.log').write_text(stdout)
    output = json.loads(stdout)
    result = dict(side=side, late=late, invocation=invocation, identity=identity, request=request, initial_wait=initial_wait, interruptions=receipts, elapsed=ended-started, exit_code=process.returncode, output=output, requests=peer.rows, recipient_errors=peer.errors)
    assert process.returncode == 0 and len(peer.rows) == 1, result
    if not late:
      result['satisfied'] = output.get('result') == 'owned-session' and ended-started < 12
    else:
      result['satisfied'] = 'time' in output.get('error', '').lower() and 11.5 <= ended-started < 14 and len(receipts) == 2
    (root / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
    return result
  finally:
    stop_child(process, pid)
    peer.close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--case', choices=['before_deadline', 'after_deadline', 'both'], default='both')
  args = parser.parse_args()
  free = shutil.disk_usage(args.output.parent).free
  args.output.mkdir(exist_ok=False)
  results = []
  for late in ([False, True] if args.case == 'both' else [args.case == 'after_deadline']):
    for side in ['source', 'native']:
      result = run(side, args.binary, args.output / ('after_deadline' if late else 'before_deadline') / side, late)
      results.append(result)
      print(side, 'after_deadline' if late else 'before_deadline', json.dumps(result['output']), round(result['elapsed'], 3), 'satisfied', result['satisfied'], flush=True)
  rows = dict(passed=all(r['satisfied'] for r in results), results=results, free_bytes=free, binary=dict(path=str(args.binary), sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest()), original_sha256=hashlib.sha256((ROOT / 'openpilot/selfdrive/carrot/web_upload.py').read_bytes()).hexdigest())
  (args.output / 'result.json').write_text(json.dumps(rows, indent=2) + '\n')
  raise SystemExit(0 if rows['passed'] else 1)


if __name__ == '__main__':
  main()
