#!/usr/bin/env python3
import argparse
import ctypes
import json
import os
from pathlib import Path
import signal
import tempfile
import time

from check_dashcam_runtime import Peer, Receiver


def children(pid: int) -> list[int]:
  path = Path(f'/proc/{pid}/task/{pid}/children')
  return [int(value) for value in path.read_text().split()] if path.exists() else []


def wait_for(predicate, description: str) -> None:
  deadline = time.monotonic() + 5
  while not predicate():
    assert time.monotonic() < deadline, description
    time.sleep(0.01)


def run(binary: Path, output: Path, scenario: str) -> dict:
  output.mkdir(parents=True)
  receiver = Receiver('normal' if scenario == 'idle-success' else 'stale')
  peer = Peer([str(binary)], output / 'process.log')
  worker = None
  try:
    with tempfile.TemporaryDirectory(prefix='dashcam-lifecycle-') as temp:
      root = Path(temp)
      segment = '00000001--1234567890--0'
      (root / segment).mkdir()
      (root / segment / 'rlog.zst').write_bytes(b'fixture rlog')
      settings = {
        'root': str(root),
        'base_url': receiver.base,
        'token': 'fixture-session',
        'metadata': {'carName': 'fixture', 'dongleId': 'fixture-device'},
        'webhook': receiver.base + '/webhook',
        'concurrency': 1,
      }
      start = peer.call(op='start', root=str(root), segments=[segment], settings=settings)
      assert start['ok'], start
      wait_for(lambda: len(children(peer.process.pid)) == 1, 'worker was not spawned')
      worker = children(peer.process.pid)[0]
      assert receiver.started.wait(5), 'upload did not start'
      if scenario == 'idle-success':
        wait_for(lambda: any(capture['path'] == '/webhook' for capture in receiver.captures), 'upload never completed')
        wait_for(lambda: not children(peer.process.pid), 'completed worker was not reaped without another API call')
        final = peer.call(op='snapshot', id=start['job_id'])
        assert final['status'] == 'done' and final['result']['ok'], final
      elif scenario == 'worker-killed':
        os.kill(worker, signal.SIGKILL)
        wait_for(lambda: not children(peer.process.pid), 'killed worker was not reaped')
        final = peer.call(op='snapshot', id=start['job_id'])
        assert final['done'] and not final['result']['ok'] and final['status'] == 'failed', final
      elif scenario == 'owner-eof':
        peer.close()
        assert not Path(f'/proc/{worker}').exists(), 'worker survived manager drop'
      else:
        peer.process.send_signal(signal.SIGTERM if scenario == 'owner-term' else signal.SIGKILL)
        peer.process.wait(timeout=5)
        assert receiver.disconnected.wait(2), 'worker connection survived owner death'
        waited, status = os.waitpid(worker, 0)
        assert waited == worker and os.waitstatus_to_exitcode(status) == -signal.SIGKILL, status
      if scenario != 'idle-success':
        assert receiver.disconnected.wait(2), 'worker connection was not closed'
        assert not any(capture['path'].endswith('/complete') or capture['path'] == '/webhook' for capture in receiver.captures)
      result = {'scenario': scenario, 'passed': True, 'manager_pid': peer.process.pid, 'worker_pid': worker}
      (output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
      return result
  finally:
    if peer.process.poll() is None:
      peer.close()
    else:
      peer.log.close()
    if worker is not None and worker in children(os.getpid()):
      os.kill(worker, signal.SIGKILL)
      os.waitpid(worker, 0)
    receiver.close()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--scenario', choices=['idle-success', 'worker-killed', 'owner-eof', 'owner-term', 'owner-kill'])
  args = parser.parse_args()
  libc = ctypes.CDLL(None, use_errno=True)
  libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
  libc.prctl.restype = ctypes.c_int
  assert libc.prctl(36, 1, 0, 0, 0) == 0, ctypes.get_errno()
  scenarios = [args.scenario] if args.scenario else ['idle-success', 'worker-killed', 'owner-eof', 'owner-term', 'owner-kill']
  reports = [run(args.binary.resolve(), args.output / scenario, scenario) for scenario in scenarios]
  (args.output / 'report.json').write_text(json.dumps(reports, indent=2) + '\n')
  print(json.dumps({'passed': len(reports)}))


if __name__ == '__main__':
  main()
