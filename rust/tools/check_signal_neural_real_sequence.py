"""Actual pinned CPU inference plus causal neural sequence, with frozen RGB inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import cv2
import numpy as np
from signal_source import classes
from check_signal_neural_sequence import frozen


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--models', type=Path, required=True)
  parser.add_argument('--images', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--active-image', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  scope = classes('tools/signal_analysis/neural_signal.py', ['NeuralSignal', 'NeuralSignalSequence'])
  scope.update(np=np, cv2=cv2, Path=Path, hashlib=hashlib)
  scope['overlap'] = classes('tools/signal_analysis/signal_tracker.py', ['overlap'])['overlap']
  network = scope['NeuralSignal'](args.models / 'detector.onnx', args.models / 'classifier.onnx', backend='cpu')
  source = json.loads(args.images.read_text())['cases']
  cases, expected = [], []
  for scenario in range(2):
    sequence = scope['NeuralSignalSequence'](network)
    frames, results = [], []
    now = 10.
    for index in range(8):
      now += .3 if scenario == 1 and index == 5 else .05
      if scenario == 1 and index == 7:
        now -= 1.
      image = source[(2, 2, 3, 3, 2, 1, 2, 3)[index]]
      if args.active_image:
        image = dict(image, path=str(args.active_image.resolve()))
      rgb = np.frombuffer(Path(image['path']).read_bytes(), np.uint8).reshape(image['height'], image['width'], 3)
      frames.append(dict(path=image['path'], width=image['width'], height=image['height'], timestamp=now))
      result = sequence.process(rgb, now)
      state = {k: v for k, v in vars(sequence).items() if k != 'neural'}
      results.append(frozen(dict(result=result, state=state)))
    cases.append(frames)
    expected.append(results)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  print(json.dumps(dict(frames=sum(map(len, cases)), tracks=sum(len(r['result']['tracks']) for s in expected for r in s), native_compared=False)))


if __name__ == '__main__':
  main()
