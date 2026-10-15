"""Actual final neural preprocessing/box/confidence/tie/classifier boundaries."""
import argparse
import hashlib
import json
from pathlib import Path
import cv2
import numpy as np
from signal_source import classes


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  scope = classes('tools/signal_analysis/neural_signal.py', ['NeuralSignal'])
  scope.update(np=np, cv2=cv2)
  scope['overlap'] = classes('tools/signal_analysis/signal_tracker.py', ['overlap'])['overlap']
  cases, expected = [], []
  for index in range(96):
    network = object.__new__(scope['NeuralSignal'])
    network.grid = np.array([(x, y, s) for s in (8, 16, 32) for y in range(416//s) for x in range(416//s)], np.float32)
    rgb = np.random.default_rng(index).integers(0, 256, (48 + index % 3, 64 + index % 5, 3), np.uint8)
    prediction = np.zeros((1, 3549, 8), np.float32)
    rows = []
    for n in range((1, 2, 3, 17, 32, 65)[index % 6]):
      slot = (n * 47 + index * 11) % 3549
      q = prediction[0, slot]
      q[0:2] = ((n % 8) - 3.5, (n % 5) - 2.)
      q[2:4] = (.5 + (n % 3), 1. + (n % 4))
      q[4] = .5
      q[6] = [.6, np.nextafter(np.float32(.6), np.float32(1.)), .9, 1.][index % 4] if index < 48 else .9
      if index >= 72:
        q[6] = (.7, .8, .9)[n % 3]
      if index % 11 == 0 and n % 3 == 0:
        q[2] = 40.
      rows.append(dict(index=slot, values=q.tolist()))
    probabilities = np.arange(11, dtype=np.float32) / np.float32(100.)
    probabilities[index % 11] = .8
    if index % 5 == 0:
      probabilities[(index + 3) % 11] = .8
    captures = []
    class Fake:
      def __init__(self, output):
        self.output = output
      def run(self, _, values):
        name, tensor = next(iter(values.items()))
        captures.append(dict(name=name, shape=list(tensor.shape), sha256=hashlib.sha256(tensor.tobytes()).hexdigest()))
        return [self.output]
    network.detector, network.classifier = Fake(prediction), Fake(probabilities[None])
    rect = (3, 2, rgb.shape[1] - 2, rgb.shape[0] - 1) if index % 2 else None
    bounds = [[1., 1., 23., 14.], [-2., -1., 31., 20.], [0., 0., 0., 0.], [1.9, 1.1, 33.8, 34.2]][index % 4]
    proposals = network.detect(rgb, rect)
    color = network.classify(rgb, bounds)
    path = args.output / f'{index}.rgb'
    path.write_bytes(rgb.tobytes())
    cases.append(dict(path=str(path.resolve()), width=rgb.shape[1], height=rgb.shape[0], rect=rect, box=bounds, rows=rows, probabilities=probabilities.tolist()))
    expected.append(dict(proposals=proposals, color=color, captures=captures))
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  print(json.dumps(dict(cases=len(cases), proposals=sum(len(v['proposals']) for v in expected), captures=sum(len(v['captures']) for v in expected), scope='actual_source_scripted_logits_no_inference', native_compared=False)))


if __name__ == '__main__':
  main()
