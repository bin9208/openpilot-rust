"""Replay original tracker/publishers against a connected native worker capture.

The native child already used real VisionIPC/files/subscriptions. Reference
clocks, frame identity and session are captured inputs; this is not vehicle timing.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile
from types import ModuleType
import cv2
import numpy as np
from check_signal_publication import publisher
from signal_source import source, compare


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--input', type=Path, required=True)
  parser.add_argument('--capture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  inputs = json.loads(args.input.read_text())
  rows = [row for row in json.loads((args.capture / 'trace.json').read_text()) if row.get('phase', 'live') == 'live']
  observed = [v['fields'] for v in json.loads((args.capture / 'observations.json').read_text()) if v['event'] == 'signalTrackingShadow']
  assert len(rows) == len(observed) == len(inputs['images']) == 24
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  module = ModuleType('signal_tracker')
  sys.modules[module.__name__] = module
  exec(compile(source('tools/signal_analysis/signal_tracker.py'), 'e8e152db:signal_tracker', 'exec'), module.__dict__)
  tracker = module.SignalTracker(daytime_cores=True, robust_tracking=True, verified_lamps=True)
  writers = {name: publisher('openpilot/selfdrive/carrot/' + file, function) for name, file, function in [
    ('start', 'signal_start_guard.py', 'publish_start_observation'),
    ('prepare', 'signal_stop_prepare_runtime.py', 'publish_prepare_observation'),
  ]}
  expected, actual = [], []
  with tempfile.TemporaryDirectory(prefix='signal-live-source-') as temporary:
    root = Path(temporary)
    for row, record, image in zip(rows, observed, inputs['images'], strict=True):
      rgb = np.fromfile(image['rgb'], np.uint8).reshape(image['height'], image['width'], 3)
      assert hashlib.sha256(rgb.tobytes()).hexdigest() == image['rgb_sha256']
      result = tracker.process(rgb, record['timestamp_eof'] / 1e9, allow_green=True)
      reference_record = dict(record, tracks=result['tracks'])
      payloads = {}
      for name, function in writers.items():
        function.__globals__['requested'] = lambda _path: True
        path = root / (name + '.json')
        function(reference_record, directory=root, path=path)
        payloads[name] = json.loads(path.read_text())
      expected.append(dict(tracks=result['tracks'], start=payloads['start'], prepare=payloads['prepare']))
      actual.append(dict(tracks=record['tracks'], start=row['start'], prepare=row['prepare']))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  (args.output / 'actual.json').write_text(json.dumps(actual))
  receipt = compare(actual, expected)
  receipt.update(frames=24, source_commit=inputs['source_commit'], scope='actual-capture original tracker and publishers; consumer owner checked separately')
  (args.output / 'receipt.json').write_text(json.dumps(receipt) + '\n')
  print(json.dumps(receipt))


if __name__ == '__main__':
  main()
