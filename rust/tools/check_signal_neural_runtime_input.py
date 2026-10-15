"""Actual final NV12 -> neural sequence -> display/control payload reference."""
import argparse
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import cv2
import numpy as np
from signal_source import classes
from check_signal_publication import publisher


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--nv12', type=Path, required=True)
  parser.add_argument('--models', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  unpack = classes('openpilot/selfdrive/modeld/signal_tracking_shadow.py', ['copy_nv12_rgb'])
  unpack['REFERENCE_SIZE'] = (1344, 760)
  data = args.nv12.read_bytes()
  rgb = unpack['copy_nv12_rgb'](SimpleNamespace(width=1344, height=760, stride=1344, uv_offset=1344*760, data=data))
  scope = classes('tools/signal_analysis/neural_signal.py', ['NeuralSignal', 'NeuralSignalSequence'])
  scope.update(np=np, cv2=cv2, Path=Path, hashlib=hashlib)
  scope['overlap'] = classes('tools/signal_analysis/signal_tracker.py', ['overlap'])['overlap']
  network = scope['NeuralSignal'](args.models / 'detector.onnx', args.models / 'classifier.onnx', backend='cpu')
  result = scope['NeuralSignalSequence'](network).process(rgb, 10.)
  display_tracks = classes('openpilot/selfdrive/modeld/signal_neural_shadow.py', ['display_tracks'])['display_tracks']
  tracks = display_tracks(result['tracks'])
  record = dict(neural=True, backend='cpu', timestamp_eof=10_000_000_000, frame_id=1, session='source', tracks=tracks)
  payloads = {}
  for name, path, function in [('start', 'openpilot/selfdrive/carrot/signal_start_guard.py', 'publish_start_observation'), ('display', 'openpilot/selfdrive/carrot/signal_display.py', 'publish_display')]:
    output = args.output / (name + '.json')
    publisher(path, function)(record, path=output)
    payloads[name] = json.loads(output.read_text())
  receipt = dict(nv12_sha256=hashlib.sha256(data).hexdigest(), rgb_sha256=hashlib.sha256(rgb.tobytes()).hexdigest(), tracks=tracks, payloads=payloads, native_compared=False)
  (args.output / 'expected.json').write_text(json.dumps(receipt))
  print(json.dumps(dict(tracks=len(tracks), labels=[t['label'] for t in tracks], raw=[t['evidence']['raw'] for t in tracks], scope='actual NV12/model/sequence/publish source; not worker loop')))


if __name__ == '__main__':
  main()
