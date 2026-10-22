from __future__ import annotations

import argparse
import importlib.util
import os
import json
import signal
from pathlib import Path
import sys
import threading
import time


def fixture_fence_due(path: Path, frame: int, limit: int) -> bool:
  try:
    text = path.read_text().strip()
  except FileNotFoundError:
    return False
  digits = text[1:] if text.startswith(('+', '-')) else text
  if not digits.isascii() or not digits.isdecimal():
    raise ValueError('fixture phase fence requires an ASCII frame integer')
  target = int(text)
  if target < 0 or target >= limit or frame > target:
    raise ValueError('fixture phase fence is outside the remaining step bound')
  return frame == target


def load_binding(path: Path) -> None:
  spec = importlib.util.spec_from_file_location('openpilot.common.params_pyx', path)
  if spec is None or spec.loader is None:
    raise RuntimeError('source Params binding is unavailable')
  module = importlib.util.module_from_spec(spec)
  sys.modules[spec.name] = module
  spec.loader.exec_module(module)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--params-root', type=Path, required=True)
  parser.add_argument('--steps', type=int, required=True)
  parser.add_argument('--frequency-trace', type=Path)
  parser.add_argument('--fixture-phase-fence', type=Path)
  arguments = parser.parse_args()
  if arguments.fixture_phase_fence and (arguments.frequency_trace is None or arguments.steps <= 0):
    parser.error('fixture phase fence requires frequency trace and a fixture step bound')
  load_binding(arguments.binding)
  assert os.environ['PARAMS_ROOT'] == str(arguments.params_root)
  from openpilot.selfdrive.car.card import Car
  from openpilot.common.realtime import config_realtime_process, Priority
  config_realtime_process(5, Priority.CTRL_HIGH)
  runtime = Car()
  stopped = threading.Event()
  reader = threading.Thread(target=runtime.params_thread, args=(stopped,))
  previous = None
  previous_record_finished_ns = None
  observer = arguments.frequency_trace.open('w') if arguments.frequency_trace else None
  try:
    reader.start()
    for _ in range(arguments.steps):
      started = (time.monotonic_ns(), time.thread_time_ns()) if observer is not None else None
      runtime.step()
      if observer is not None:
        step_finished_ns, thread_cpu_finished_ns = time.monotonic_ns(), time.thread_time_ns()
        record_started_ns = time.monotonic_ns()
        tracker = runtime.sm.freq_tracker['carControl']
        received = runtime.sm.recv_time['carControl']
        interval = None
        if runtime.sm.updated['carControl']:
          interval = None if previous is None else received - previous
          previous = received
        parser = None
        if runtime.CP.brand == 'hyundai':
          parser = {'counter': runtime.CI.CS.controls_ready_count, 'pt_ready': runtime.CI.CS.cp.controls_ready,
                    'cam_ready': runtime.CI.CS.cp_cam.controls_ready}
        sample = {'frame': runtime.sm.frame, 'receive_time': received, 'interval': interval,
          'average_frequency': 1. / tracker.avg_dt.get_average() if tracker.avg_dt.count else None,
          'min_frequency': tracker.min_freq, 'max_frequency': tracker.max_freq, 'tracker_valid': tracker.valid,
          'frequency_ok': runtime.sm.freq_ok['carControl'], 'controls_ready': runtime.params.get_bool('ControlsReady'), 'parser': parser}
        sample['admission'] = [
          {'name': name, 'seen': runtime.sm.seen[name], 'updated': runtime.sm.updated[name],
           'receive_frame': runtime.sm.recv_frame[name], 'log_mono_time': runtime.sm.logMonoTime[name]}
          for name in runtime.sm.services]
        sample['timing'] = {'step_started_ns': started[0], 'step_finished_ns': step_finished_ns,
          'thread_cpu_started_ns': started[1], 'thread_cpu_finished_ns': thread_cpu_finished_ns,
          'receive_ns': runtime.card_diag_recv_ns, 'stage_us': dict(runtime.card_diag_stage_current)}
        sample['record_started_ns'] = record_started_ns
        sample['previous_record_finished_ns'] = previous_record_finished_ns
        observer.write(json.dumps(sample) + '\n')
        observer.flush()
        previous_record_finished_ns = time.monotonic_ns()
      runtime.rk.monitor_time()
      if arguments.fixture_phase_fence and fixture_fence_due(arguments.fixture_phase_fence, runtime.sm.frame, arguments.steps):
        observer.flush()
        os.kill(os.getpid(), signal.SIGSTOP)
  finally:
    stopped.set()
    reader.join()
    if observer:
      observer.close()


if __name__ == '__main__':
  main()
