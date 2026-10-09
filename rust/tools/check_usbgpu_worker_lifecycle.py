from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
import ctypes
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from types import SimpleNamespace

import numpy as np


def client(args, mode):
  from openpilot.selfdrive.modeld import precompiled_runner as original
  from openpilot.selfdrive.modeld import precompiled_model

  popen = subprocess.Popen
  record = precompiled_model.record_failure
  failures, invocations = [], []

  def launch(command, **kwargs):
    command = [str(args.binary), str(args.metadata), *command[-3:], str(args.evidence / f'{mode}-inputs.jsonl')]
    kwargs['env'] = {**os.environ, 'SEND_RAW_PRED': '1'}
    if mode != 'thread-loader':
      kwargs['env'][f'USBGPU_WORKER_TEST_{mode.upper()}'] = '1'
    invocations.append(command)
    return popen(command, **kwargs)

  original.subprocess.Popen = launch
  precompiled_model.record_failure = lambda path, error, phase: failures.append({'phase': phase, 'error': str(error)})
  model = None
  error_text = None
  try:
    with ThreadPoolExecutor(max_workers=1) as pool:
      model = pool.submit(original.PrecompiledModelState, 1344, 760, args.metadata).result()
    process, shared = model.process, Path(model.file.name)
    frames = {name: SimpleNamespace(data=np.zeros(model.frame_size, np.uint8)) for name in ('img', 'big_img')}
    transforms = {name: np.eye(3, dtype=np.float32) for name in frames}
    inputs = {'desire_pulse': np.zeros(8, np.float32), 'traffic_convention': np.array([1, 0], np.float32),
              'action_t': np.array([0.05, 0.05], np.float32)}
    if mode == 'disconnect':
      model.run(frames, transforms, inputs, False)
      before = shared.read_bytes()[model.views['img'].ctypes.data - model.views['tfm'].ctypes.data + 2 * model.frame_size:]
    try:
      result = model.run(frames, transforms, inputs, True)
      assert mode == 'thread-loader' and np.isfinite(result['plan']).all()
    except RuntimeError as error:
      error_text = str(error)
      assert mode != 'thread-loader'
    if mode == 'disconnect':
      assert before[:4] == np.float32(1).tobytes()
    model.close()
    model = None
    assert process.poll() is not None and not shared.exists()
    if mode == 'thread-loader':
      assert not failures and error_text is None
    else:
      assert error_text and len(failures) == 1 and failures[0]['phase'] == 'inference'
    return {'scenario': mode, 'invocations': invocations, 'error': error_text, 'failures': failures,
            'worker_reaped': process.poll() is not None, 'shared_file_removed': not shared.exists(), 'passed': True}
  finally:
    if model is not None:
      model.close()
    original.subprocess.Popen = popen
    precompiled_model.record_failure = record


def parent_death(args):
  libc = ctypes.CDLL(None, use_errno=True)
  if libc.prctl(36, 1, 0, 0, 0) != 0:
    raise OSError(ctypes.get_errno(), 'PR_SET_CHILD_SUBREAPER')
  read_fd, write_fd = os.pipe()
  command = [str(args.binary), str(args.metadata), str(args.evidence / 'orphan-shared'), '1344', '760',
             str(args.evidence / 'orphan-inputs.jsonl')]
  (args.evidence / 'orphan-shared').touch()
  helper = ('import subprocess,sys; p=subprocess.Popen(sys.argv[2:],stdin=int(sys.argv[1]),stdout=subprocess.PIPE); ' +
            'ready=p.stdout.readline(); print(str(p.pid),flush=True); print(ready.decode(),flush=True)')
  started = time.monotonic()
  parent = subprocess.run([sys.executable, '-c', helper, str(read_fd), *command], pass_fds=(read_fd,),
                          capture_output=True, text=True, timeout=5, check=True)
  os.close(read_fd)
  pid = int(parent.stdout.splitlines()[0])
  status = None
  try:
    while time.monotonic() - started < 5:
      found, observed = os.waitpid(pid, os.WNOHANG)
      if found:
        status = os.waitstatus_to_exitcode(observed)
        break
      time.sleep(0.02)
    assert status == 1
  finally:
    os.close(write_fd)
    if status is None:
      os.kill(pid, signal.SIGKILL)
      os.waitpid(pid, 0)
    libc.prctl(36, 0, 0, 0, 0)
  return {'scenario': 'parent-death-with-control-pipe-still-open', 'invocation': command, 'pid': pid,
          'exit_code': status, 'seconds': time.monotonic() - started, 'ready': json.loads(parent.stdout.splitlines()[1]),
          'passed': status == 1}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--metadata', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
  rows = []
  for mode in ['thread-loader', 'nonfinite', 'disconnect']:
    rows.append(client(args, mode))
    (args.evidence / f'{mode}.json').write_text(json.dumps(rows[-1], indent=2) + '\n')
  rows.append(parent_death(args))
  (args.evidence / 'comparison.json').write_text(json.dumps({'passed': True, 'rows': rows}, indent=2) + '\n')
  print(json.dumps({'passed': True, 'scenarios': len(rows)}))


if __name__ == '__main__':
  main()
