"""Host-only regressions for attribution; no project Params/msgq/Car imports."""
from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from types import ModuleType, SimpleNamespace

import pytest

import card_runtime_source
from card_qa.runtime_pumped_shutdown import finish


def shutdown_peer(tmp_path, monkeypatch, process):
  # Only the IPC boundary is replaced; finish's empty-CAN/signal/wait path runs unchanged.
  boundary = ModuleType('card_qa.runtime_pumped_capture')
  boundary.drain = lambda _: {name: [] for name in ('carState', 'carOutput', 'carParams', 'sendcan')}
  def set_phase(*_):
    pytest.fail('failed runtime shutdown must not advance the pump phase')
  boundary.set_phase = set_phase
  monkeypatch.setitem(sys.modules, boundary.__name__, boundary)
  (tmp_path / 'frequency.jsonl').write_text('{"frame":1}\n')
  def state(stamp, count, errors):
    return {'logMonoTime': stamp, 'valid': True, 'carState': {'canErrorCounter': errors,
      'radarInput': {'firstCanMonoTime': stamp if count else 0, 'lastCanMonoTime': stamp if count else 0,
                     'canPacketCount': count, 'receiveMonoTime': stamp}}}
  stream = {'carState': [state(100, 1, 0), state(200, 0, 1)], 'carOutput': [], 'carParams': [], 'sendcan': []}
  peer = SimpleNamespace(process=process, output=tmp_path, observed_frames=0, subscribers={}, pump=SimpleNamespace(pid=0))
  return peer, stream


def test_real_shutdown_timeout_keeps_original_exception_and_captures_owned_process(tmp_path, monkeypatch):
  # A removed timeout hook must lose these receipts while the same real five-second failure remains.
  code = 'import signal,time; signal.signal(signal.SIGINT,signal.SIG_IGN); print("ready",flush=True); time.sleep(30)'
  with subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE, text=True) as process:
    original_wait, original_signal = process.wait, process.send_signal
    waits, signals, errors = [], [], []
    def wait(timeout=None):
      waits.append(timeout)
      try:
        return original_wait(timeout=timeout)
      except subprocess.TimeoutExpired as error:
        errors.append(error)
        raise
    def send_signal(value):
      signals.append(value)
      original_signal(value)
    try:
      assert process.stdout.readline() == 'ready\n'
      monkeypatch.setattr(process, 'wait', wait)
      monkeypatch.setattr(process, 'send_signal', send_signal)
      peer, stream = shutdown_peer(tmp_path, monkeypatch, process)
      with pytest.raises(subprocess.TimeoutExpired) as caught:
        finish(peer, stream, True)
      assert caught.value is errors[0]
      assert waits == [5] and signals == [signal.SIGCONT, signal.SIGINT]
      assert process.poll() is None  # Diagnostics must not signal or dispose of the peer.
      assert (tmp_path / 'shutdown-timeout.json').is_file(), 'missing shutdown timeout receipt'
      receipt = json.loads((tmp_path / 'shutdown-timeout.json').read_text())
      assert receipt['timeout_seconds'] == 5 and receipt['pid'] == process.pid
      assert receipt['first_empty_can']['canErrorCounter'] == 1
      assert receipt['last_nonempty_can']['radarInput']['canPacketCount'] == 1
      assert receipt['process_capture']['status'] == 'complete'
      snapshot = json.loads((tmp_path / 'shutdown-process.json').read_text())
      assert snapshot['identity']['parent_pid'] == os.getpid()
      assert snapshot['process']['stat']['status'] == 'ok'
      assert snapshot['threads'] and all('schedstat' in row['files'] and 'stack' in row['files'] for row in snapshot['threads'])
      assert snapshot['python_stack']['status'] == 'unavailable'
      assert not (tmp_path / 'shutdown-raw.json').exists()  # No false successful lifecycle receipt.
    finally:
      monkeypatch.setattr(process, 'wait', original_wait)
      monkeypatch.setattr(process, 'send_signal', original_signal)
      process.terminate()
      original_wait(timeout=3)


@pytest.mark.parametrize('diagnostic_error', [OSError('full disk'), RuntimeError('broken collector'), KeyboardInterrupt()])
@pytest.mark.parametrize('cannot_write', [False, True])
def test_diagnostic_failures_cannot_replace_the_exact_runtime_timeout(tmp_path, monkeypatch, diagnostic_error, cannot_write):
  from card_qa import runtime_attribution
  original = subprocess.TimeoutExpired(['owned-peer'], 5)
  class Process:
    pid = os.getpid()
    def poll(self):
      return None
    def send_signal(self, _):
      pass
    def wait(self, timeout):
      assert timeout == 5
      raise original
  def fail(*_, **__):
    raise diagnostic_error
  monkeypatch.setattr(runtime_attribution, 'record_shutdown_timeout', fail)
  peer, stream = shutdown_peer(tmp_path, monkeypatch, Process())
  if cannot_write:
    def denied(*_, **__):
      raise OSError('no receipt space')
    monkeypatch.setattr(Path, 'write_text', denied)
  with pytest.raises(subprocess.TimeoutExpired) as caught:
    finish(peer, stream, False)
  assert caught.value is original
  if not cannot_write:
    assert json.loads((tmp_path / 'shutdown-diagnostic-error.json').read_text())['diagnostic_error'] == type(diagnostic_error).__name__


def test_successful_shutdown_retains_signal_order_deadlines_and_raw_receipt(tmp_path, monkeypatch):
  from card_qa import runtime_attribution
  class Process:
    pid = os.getpid()
    def poll(self):
      return None
    def send_signal(self, value):
      signals.append(value)
    def wait(self, timeout):
      assert timeout == 5
      return -signal.SIGINT
  def unexpected(*_, **__):
    pytest.fail('successful shutdown must not collect timeout diagnostics')
  monkeypatch.setattr(runtime_attribution, 'record_shutdown_timeout', unexpected)
  signals, phases = [], []
  peer, stream = shutdown_peer(tmp_path, monkeypatch, Process())
  sys.modules['card_qa.runtime_pumped_capture'].set_phase = lambda path, phase: phases.append((path, phase))
  def pump_wait(timeout):
    assert timeout == 5
    return 0
  peer.pump.wait = pump_wait
  result = finish(peer, stream, True)
  assert signals == [signal.SIGCONT, signal.SIGINT] and phases == [(tmp_path, 'stop')]
  assert result['runtime_exit'] == -signal.SIGINT and result['pump_exit'] == 0 and result['empty_can_counter_increment'] == 1
  assert json.loads((tmp_path / 'shutdown-raw.json').read_text())['result'] == result
  assert not (tmp_path / 'shutdown-timeout.json').exists()


def test_worker_timeout_retains_the_partial_shutdown_receipt(tmp_path, monkeypatch):
  from card_qa import runtime_attribution
  peer, stream = shutdown_peer(tmp_path, monkeypatch, SimpleNamespace(pid=321))
  def stalled_worker(command, **kwargs):
    # Slow proc access is the external boundary; the real receipt writer remains active.
    pending = json.loads((tmp_path / 'shutdown-timeout.json').read_text())
    assert pending['process_capture']['status'] == 'pending'
    assert kwargs['timeout'] == 1 and kwargs['stdin'] == subprocess.DEVNULL
    assert command[command.index('--pid') + 1] == '321'
    raise subprocess.TimeoutExpired(command, kwargs['timeout'])
  monkeypatch.setattr(subprocess, 'run', stalled_worker)
  runtime_attribution.record_shutdown_timeout(peer, True, subprocess.TimeoutExpired(['owned'], 5),
    stream['carState'][0], stream['carState'][1], {name: [] for name in stream})
  receipt = json.loads((tmp_path / 'shutdown-timeout.json').read_text())
  assert receipt['process_capture']['status'] == 'worker_timeout' and receipt['timeout_seconds'] == 5


def test_process_snapshot_bounds_threads_and_bytes_and_marks_unavailable(tmp_path, monkeypatch):
  from card_qa import runtime_attribution
  process_snapshot = runtime_attribution.process_snapshot
  monkeypatch.setattr(time, 'monotonic', lambda: 0.)  # Exercise byte/task bounds independently of host scheduling.
  original_open = os.open
  def readable(path, flags):
    if Path(path).name == 'stack':
      raise PermissionError(13, 'kernel stack denied', str(path))
    return original_open(path, flags)
  monkeypatch.setattr(os, 'open', readable)
  process = tmp_path / '321'
  task = process / 'task'
  task.mkdir(parents=True)
  # /proc stat's comm may contain spaces and ')'; PPID remains the fourth field.
  stat = '321 (owned ) worker) S 123 ' + '0 ' * 17 + '999\n'
  (process / 'stat').write_text(stat)
  (process / 'wchan').write_bytes(b'x' * 100_000)
  (process / 'schedstat').write_text('1 2 3\n')
  for number in range(1000, 1040):
    directory = task / str(number)
    directory.mkdir()
    for name, data in [('stat', stat), ('wchan', 'futex_wait'), ('schedstat', '1 2 3')]:
      (directory / name).write_text(data)
  result = process_snapshot(321, 123, proc_root=tmp_path)
  assert result['identity']['start_time_ticks'] == 999
  assert result['process']['wchan']['truncated'] and len(result['process']['wchan']['text']) == 4096
  assert result['process']['stack']['status'] == 'unavailable' and result['process']['stack']['errno'] == 13
  assert len(result['threads']) == 32 and result['threads_truncated']
  assert result['stored_bytes'] <= 128 * 1024
  # A recycled PID/foreign process must not trigger any thread inspection.
  foreign = process_snapshot(321, 999, proc_root=tmp_path)
  assert foreign['identity']['status'] == 'not_owned_child' and foreign['threads'] == []
  for directory in task.iterdir():
    for path in directory.iterdir():
      path.write_bytes(b'x' * 10_000)
  exhausted = process_snapshot(321, 123, proc_root=tmp_path)
  assert exhausted['stored_bytes'] == 128 * 1024 and exhausted['threads_truncated']


def test_process_snapshot_does_not_follow_an_available_stack_symlink(tmp_path):
  from card_qa.runtime_attribution import process_snapshot
  process = tmp_path / '321'
  (process / 'task').mkdir(parents=True)
  (process / 'stat').write_text('321 (owned) S 123 ' + '0 ' * 17 + '999\n')
  private = tmp_path / 'other-file'
  private.write_text('must not read')
  (process / 'stack').symlink_to(private)
  result = process_snapshot(321, 123, proc_root=tmp_path)
  assert result['process']['stack']['status'] == 'unavailable'
  assert 'must not read' not in json.dumps(result)


def test_process_snapshot_marks_disappeared_peer_without_claiming_a_stack(tmp_path):
  from card_qa.runtime_attribution import process_snapshot
  result = process_snapshot(321, 123, proc_root=tmp_path)
  assert result['identity']['status'] == 'unavailable' and result['threads'] == []
  assert result['python_stack']['status'] == 'unavailable'


def test_source_frequency_trace_preserves_fields_and_attributes_each_actual_step(tmp_path, monkeypatch):
  # Missing/late source stage capture must fail on actual main-loop JSON, not a mirrored serializer.
  modules = ('openpilot', 'openpilot.selfdrive', 'openpilot.selfdrive.car', 'openpilot.selfdrive.car.card',
             'openpilot.common', 'openpilot.common.realtime')
  for name in modules:
    monkeypatch.setitem(sys.modules, name, ModuleType(name))
  realtime = sys.modules['openpilot.common.realtime']
  realtime.Priority = SimpleNamespace(CTRL_HIGH=53)
  realtime.config_realtime_process = lambda *_: None
  instances = []
  class Car:
    def __init__(self):
      instances.append(self)
      average = SimpleNamespace(count=1, get_average=lambda: .01)
      self.sm = SimpleNamespace(frame=-1, freq_tracker={'carControl': SimpleNamespace(avg_dt=average, min_freq=90, max_freq=110, valid=True)},
        recv_time={'carControl': 1.}, updated={'carControl': True}, freq_ok={'carControl': True},
        services=['carControl'], seen={'carControl': True}, recv_frame={'carControl': 0}, logMonoTime={'carControl': 1000})
      self.CP = SimpleNamespace(brand='nissan')
      self.params = SimpleNamespace(get_bool=lambda _: True)
      self.monitors = 0
      self.rk = SimpleNamespace(monitor_time=self.monitor)
    def params_thread(self, stopped):
      stopped.wait()
    def step(self):
      self.sm.frame += 1
      self.card_diag_recv_ns = 130 + self.sm.frame * 100
      self.card_diag_stage_current = {'decode': 3, 'ci_update': 4, 'sm_update': 5, 'publish': 6}
      self.sm.recv_time['carControl'] += .01
      self.sm.recv_frame['carControl'] = self.sm.frame
      self.sm.logMonoTime['carControl'] += 1
    def monitor(self):
      self.monitors += 1
      self.card_diag_stage_current['decode'] = 999
  sys.modules['openpilot.selfdrive.car.card'].Car = Car
  monkeypatch.setattr(card_runtime_source, 'load_binding', lambda _: None)
  monkeypatch.setenv('PARAMS_ROOT', str(tmp_path))
  clocks = iter([100, 150, 160, 170, 200, 250, 260, 270])
  cpus = iter([10, 30, 40, 60])
  monkeypatch.setattr(time, 'monotonic_ns', lambda: next(clocks))
  monkeypatch.setattr(time, 'thread_time_ns', lambda: next(cpus))
  output = tmp_path / 'frequency.jsonl'
  monkeypatch.setattr(sys, 'argv', ['source', '--binding', 'unused', '--params-root', str(tmp_path), '--steps', '2', '--frequency-trace', str(output)])
  card_runtime_source.main()
  rows = [json.loads(line) for line in output.read_text().splitlines()]
  assert len(rows) == instances[0].monitors == 2
  assert rows[0]['timing'] == {'step_started_ns': 100, 'step_finished_ns': 150, 'thread_cpu_started_ns': 10,
    'thread_cpu_finished_ns': 30, 'receive_ns': 130, 'stage_us': {'decode': 3, 'ci_update': 4, 'sm_update': 5, 'publish': 6}}
  assert rows[0]['record_started_ns'] == 160 and rows[0]['previous_record_finished_ns'] is None
  assert rows[1]['record_started_ns'] == 260 and rows[1]['previous_record_finished_ns'] == 170
  assert rows[1]['timing']['receive_ns'] == 230
  assert [row['admission'][0]['log_mono_time'] for row in rows] == [1001, 1002]
  assert all(row['frequency_ok'] and row['tracker_valid'] and row['controls_ready'] for row in rows)
  assert rows[0]['interval'] is None and rows[1]['interval'] == pytest.approx(.01)
  assert all(row['average_frequency'] == 100 and row['min_frequency'] == 90 and row['max_frequency'] == 110 for row in rows)
  # Exhausted clock iterators also reject any added diagnostic clock on the untraced path.
  monkeypatch.setattr(sys, 'argv', ['source', '--binding', 'unused', '--params-root', str(tmp_path), '--steps', '1'])
  card_runtime_source.main()
  assert instances[1].monitors == 1
