from __future__ import annotations

import argparse
import importlib.util
import os
import json
from pathlib import Path
import sys
import threading


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
  arguments = parser.parse_args()
  load_binding(arguments.binding)
  assert os.environ['PARAMS_ROOT'] == str(arguments.params_root)
  from openpilot.selfdrive.car.card import Car
  from openpilot.common.realtime import config_realtime_process, Priority
  config_realtime_process(5, Priority.CTRL_HIGH)
  runtime = Car()
  stopped = threading.Event()
  reader = threading.Thread(target=runtime.params_thread, args=(stopped,))
  previous=None
  observer = arguments.frequency_trace.open('w') if arguments.frequency_trace else None
  try:
    reader.start()
    for _ in range(arguments.steps):
      runtime.step()
      if observer is not None:
        tracker=runtime.sm.freq_tracker['carControl'];received=runtime.sm.recv_time['carControl'];interval=None
        if runtime.sm.updated['carControl']: interval=None if previous is None else received-previous;previous=received
        parser=None
        if runtime.CP.brand == 'hyundai':
          parser=dict(counter=runtime.CI.CS.controls_ready_count,pt_ready=runtime.CI.CS.cp.controls_ready,cam_ready=runtime.CI.CS.cp_cam.controls_ready)
        sample=dict(frame=runtime.sm.frame,receive_time=received,interval=interval,
          average_frequency=1./tracker.avg_dt.get_average() if tracker.avg_dt.count else None,min_frequency=tracker.min_freq,max_frequency=tracker.max_freq,
          tracker_valid=tracker.valid,frequency_ok=runtime.sm.freq_ok['carControl'],controls_ready=runtime.params.get_bool('ControlsReady'),parser=parser)
        observer.write(json.dumps(sample)+'\n');observer.flush()
      runtime.rk.monitor_time()
  finally:
    stopped.set()
    reader.join()
    if observer: observer.close()


if __name__ == '__main__':
  main()
