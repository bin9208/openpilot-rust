from __future__ import annotations

from collections.abc import Callable
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from card_qa.runtime_pump import WARMUP_FRAMES
from card_qa.runtime_pumped_types import Publications, RuntimePeer, Subscriber


def arm(output: Path, frame: int) -> None:
  pending = output / 'phase-fence-next'
  pending.write_text(str(frame) + '\n')
  pending.replace(output / 'phase-fence')


def wait_stopped(process: subprocess.Popen, collect: Callable[[], None], timeout: float = 30) -> None:
  deadline = time.monotonic() + timeout
  while True:
    collect()
    pid, status = os.waitpid(process.pid, os.WUNTRACED | os.WNOHANG)
    if pid:
      assert os.WIFSTOPPED(status) and os.WSTOPSIG(status) == signal.SIGSTOP, status
      return
    assert time.monotonic() < deadline, 'completed-step fixture fence not observed'
    time.sleep(.001)


def startup(process: subprocess.Popen, pump: subprocess.Popen, subscribers: dict[str, Subscriber], output: Path) -> Publications:
  from card_qa.runtime_pumped_capture import OUTPUTS, drain, pause_pump
  rows = {name: [] for name in OUTPUTS}
  deadline = time.monotonic() + 60

  def collect() -> None:
    assert time.monotonic() < deadline, 'fenced startup setup timed out'
    assert pump.poll() is None, pump.returncode
    for name, values in drain(subscribers).items():
      rows[name].extend(values)

  wait_stopped(process, collect, timeout=60)
  pause_pump(output)
  collect()
  sends = [json.loads(line) for line in (output / 'pump/sends.jsonl').read_text().splitlines()]
  timestamps = [row['timestamp'] for row in sends if row['mode'] == 'startup']
  assert timestamps, 'no startup CAN sends'
  completed = []
  while True:
    trace = [json.loads(line) for line in (output / 'frequency.jsonl').read_text().splitlines()]
    assert trace[-1]['frame'] == len(completed)
    completed.append(trace[-1]['frame'])
    collect()
    assert len(rows['carState']) == len(rows['carOutput']) == len(completed), 'startup publication missing'
    assert rows['carParams'], 'startup CarParams publication missing'
    row = rows['carState'][-1]['carState']
    (output / 'initial-raw.json').write_text(json.dumps(rows) + '\n')
    assert row['canErrorCounter'] == 0 and row['radarInput']['canPacketCount'] > 0, row
    if row['radarInput']['lastCanMonoTime'] == timestamps[-1]:
      break
    assert row['radarInput']['lastCanMonoTime'] in timestamps
    arm(output, len(completed))
    process.send_signal(signal.SIGCONT)
    wait_stopped(process, collect)
  (output / 'startup-fence.json').write_text(json.dumps({'mode': 'completed_step_fixture_setup',
    'completed_frames': completed, 'startup_sends': len(timestamps),
    'last_send_monotonic_ns': timestamps[-1], 'observed_stop_monotonic_ns': time.monotonic_ns(),
    'startup_states': len(rows['carState'])}) + '\n')
  return rows


def warmup(context: RuntimePeer) -> Publications:
  from card_qa.runtime_pumped_capture import OUTPUTS, drain, pause_pump, set_phase
  output = context.output
  target = context.observed_frames + WARMUP_FRAMES - 1
  arm(output, context.observed_frames)
  (output / 'warmup-start-frame').write_text(str(context.observed_frames))
  set_phase(output, 'warmup')
  deadline = time.monotonic() + 30
  rows = {name: [] for name in OUTPUTS}

  def collect() -> None:
    assert time.monotonic() < deadline, 'fenced warmup setup timed out'
    assert context.pump.poll() is None, context.pump.returncode
    for name, values in drain(context.subscribers).items():
      rows[name].extend(values)

  completed = []
  for tick in range(WARMUP_FRAMES):
    while True:
      sends = (output / 'pump/sends.jsonl').read_text().split('\n')[:-1]
      last = json.loads(sends[-1]) if sends else None
      if last and last['mode'] == 'warmup' and last['tick'] == tick:
        break
      collect()
      time.sleep(.001)
    context.process.send_signal(signal.SIGCONT)
    wait_stopped(context.process, collect)
    sample = json.loads((output / 'frequency.jsonl').read_text().splitlines()[-1])
    assert sample['frame'] == context.observed_frames + tick
    completed.append(sample['frame'])
    if tick + 1 < WARMUP_FRAMES:
      arm(output, context.observed_frames + tick + 1)
  observed_stop = time.monotonic_ns()
  pause_pump(output)
  collect()
  (output / 'warmup-raw.json').write_text(json.dumps(rows) + '\n')
  trace = (output / 'frequency.jsonl').read_text().splitlines()
  sample = json.loads(trace[-1])
  sends = [json.loads(line) for line in (output / 'pump/sends.jsonl').read_text().splitlines()]
  warmup_sends = [row for row in sends if row['mode'] == 'warmup']
  (output / 'warmup-fence.json').write_text(json.dumps({'target': target, 'completed_frame': sample['frame'],
    'mode': 'completed_step_fixture_setup', 'completed_frames': completed,
    'observed_stop_monotonic_ns': observed_stop, 'last_send_monotonic_ns': warmup_sends[-1]['timestamp'],
    'warmup_sends': len(warmup_sends), 'warmup_states': len(rows['carState'])}) + '\n')
  assert sample['frame'] == target
  assert len(warmup_sends) == len(rows['carState']) == WARMUP_FRAMES
  assert sample['frequency_ok'] and (context.passive or sample['controls_ready'])
  if context.candidate == 'GENESIS_G70':
    assert sample['parser']['counter'] > 200 and sample['parser']['pt_ready'] and sample['parser']['cam_ready']
  timestamps = [row['timestamp'] for row in warmup_sends]
  received = [row['carState']['radarInput']['lastCanMonoTime'] for row in rows['carState']]
  assert received == timestamps, [(index, expected, actual) for index, (expected, actual) in enumerate(zip(timestamps, received, strict=True))
                                 if expected != actual]
  for timestamp, row in zip(timestamps, rows['carState'], strict=True):
    radar = row['carState']['radarInput']
    assert radar['firstCanMonoTime'] == radar['lastCanMonoTime'] == timestamp and radar['canPacketCount'] == 1
    assert row['carState']['canErrorCounter'] == rows['carState'][0]['carState']['canErrorCounter']
  diagnostic_count = 10 if context.candidate == 'HONDA_CRV_5G' else 0
  diagnostic = [{'address': 0x18DAB0F1, 'deprecated': {'busTime': 0}, 'dat': [2, 0x10, 3, 0, 0, 0, 0, 0], 'src': 1}]
  for row in rows['sendcan'][:diagnostic_count]:
    assert row['valid'] and row['sendcan'] == diagnostic
  assert len(rows['sendcan']) == (0 if context.passive else WARMUP_FRAMES + diagnostic_count)
  (output / 'phase-fence').unlink()
  return rows
