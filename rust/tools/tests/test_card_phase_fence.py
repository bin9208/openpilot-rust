from __future__ import annotations

from pathlib import Path
import json
import os
import signal
import subprocess
import sys
import time

import pytest
from card_runtime_source import fixture_fence_due
from card_qa.runtime_phase_fence import wait_stopped


def test_explicit_frame_fence_rejects_invalid_and_missed_targets(tmp_path: Path) -> None:
  path = tmp_path / 'fence'
  assert not fixture_fence_due(path, 4, 10)
  path.write_text('5\n')
  assert not fixture_fence_due(path, 4, 10)
  assert fixture_fence_due(path, 5, 10)
  with pytest.raises(ValueError):
    fixture_fence_due(path, 6, 10)
  for invalid in ('-1', '10', 'invalid', '', '5_0', '５'):
    path.write_text(invalid)
    with pytest.raises(ValueError):
      fixture_fence_due(path, 4, 10)
  path.unlink()
  assert not fixture_fence_due(path, 6, 10)


def test_observer_collects_after_real_child_self_stop_before_resuming() -> None:
  code = 'import os,signal; print("complete",flush=True); os.kill(os.getpid(),signal.SIGSTOP); print("resumed",flush=True)'
  with subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE, text=True) as process:
    try:
      wait_stopped(process, lambda: None)
      time.sleep(.05)
      assert process.poll() is None
      assert process.stdout.readline() == 'complete\n'
      process.send_signal(signal.SIGCONT)
      assert process.stdout.readline() == 'resumed\n'
      assert process.wait(timeout=3) == 0
    finally:
      if process.poll() is None:
        process.send_signal(signal.SIGCONT)
        process.kill()
        process.wait(timeout=3)


def test_independent_pump_resumes_owned_receiver_before_late_observer(tmp_path: Path) -> None:
  receiver_code = '''import json,os,select,signal
os.kill(os.getpid(),signal.SIGSTOP)
counts=[]
pending=b''
while sum(counts)<80:
  pending+=os.read(0,4096)
  while select.select([0],[],[],0)[0]:
    pending+=os.read(0,4096)
  packets=pending.split(b'\\n')
  pending=packets.pop()
  counts.append(len(packets))
print(json.dumps(counts),flush=True)
'''
  pump_code = '''import os,runpy,sys,types
class Publisher:
  def send(self,data): os.write(1,data+b'\\n')
cereal=types.ModuleType('openpilot.cereal')
cereal.messaging=types.SimpleNamespace(pub_sock=lambda name:Publisher())
sys.modules['openpilot']=types.ModuleType('openpilot')
sys.modules['openpilot.cereal']=cereal
runtime=types.ModuleType('check_card_runtime')
runtime.can_packet=lambda frames,timestamp:str(timestamp).encode()
sys.modules['check_card_runtime']=runtime
sys.argv=sys.argv[1:]
runpy.run_path(sys.argv[0],run_name='__main__')
'''
  output = tmp_path / 'receiver'
  output.mkdir()
  control = output / 'pump-control'
  control.write_text('pause')
  inputs = output / 'inputs.json'
  inputs.write_text(json.dumps([{'messages': {}, 'frames': []}] * 81))
  read_fd, write_fd = os.pipe()
  receiver = subprocess.Popen([sys.executable, '-c', receiver_code], stdin=read_fd, stdout=subprocess.PIPE, text=True)
  os.close(read_fd)
  pump = None
  try:
    wait_stopped(receiver, lambda: None)
    (output / 'runtime-pid').write_text(str(receiver.pid))
    script = Path(__file__).resolve().parents[1] / 'card_qa/runtime_pump.py'
    pump = subprocess.Popen([sys.executable, '-c', pump_code, str(script), '--inputs', str(inputs),
      '--control', str(control), '--evidence', str(output / 'pump'), '--prefix', 'owned-release'], stdout=write_fd,
      stderr=subprocess.PIPE, text=True)
    os.close(write_fd)
    deadline = time.monotonic() + 3
    while not (output / 'pump/ready').exists():
      assert pump.poll() is None
      assert time.monotonic() < deadline
      time.sleep(.001)
    pending_control = output / 'pump-control-next'
    pending_control.write_text('stream')
    pending_control.replace(control)
    time.sleep(.13)
    receiver.send_signal(signal.SIGCONT)
    stdout, _ = receiver.communicate(timeout=3)
    assert receiver.returncode == 0
    pending_control.write_text('stop')
    pending_control.replace(control)
    _, stderr = pump.communicate(timeout=3)
    assert pump.returncode == 0, stderr
    sends = [json.loads(line) for line in (output / 'pump/sends.jsonl').read_text().splitlines()]
    assert len(sends) == 80
    counts = json.loads(stdout)
    assert sum(counts) == len(counts) == 80, counts
  finally:
    if pump is not None and pump.poll() is None:
      pump.kill()
      pump.wait(timeout=3)
    if receiver.poll() is None:
      receiver.send_signal(signal.SIGCONT)
      receiver.kill()
      receiver.wait(timeout=3)
