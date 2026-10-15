"""Actual final neural sequence with scripted network results; not inference evidence."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
import cv2
import numpy as np
from signal_source import classes, compare


def frozen(value):
  if isinstance(value, np.ndarray):
    return dict(shape=list(value.shape), dtype=str(value.dtype), sha256=hashlib.sha256(value.tobytes()).hexdigest())
  if isinstance(value, dict):
    return {k: frozen(v) for k, v in value.items()}
  if isinstance(value, (tuple, list)):
    return [frozen(v) for v in value]
  return value


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  overlap = classes('tools/signal_analysis/signal_tracker.py', ['overlap'])['overlap']
  scope = classes('tools/signal_analysis/neural_signal.py', ['NeuralSignalSequence'])
  scope.update(np=np, cv2=cv2, overlap=overlap)
  cases, expected = [], []
  for scenario in range(8):
    class Network:
      proposals = []
      calls = []
      tick = 0
      def detect(self, rgb, rect):
        self.calls.append(dict(kind='detect', rect=list(rect)))
        return json.loads(json.dumps(self.proposals))
      def classify(self, rgb, box):
        self.calls.append(dict(kind='classify', box=box))
        return dict(label=('red', 'green', 'unknown', 'left,red')[self.tick % 4], score=(.9, .8, .1, .95)[self.tick % 4])
    network = Network()
    sequence = scope['NeuralSignalSequence'](network)
    rng = np.random.default_rng(163 + scenario)
    base = rng.integers(0, 256, (128, 192, 3), np.uint8)
    timestamp, frames, outputs = 10., [], []
    for index in range(24):
      timestamp += .3 if scenario == 3 and index in (8, 16) else .05
      if scenario == 4 and index == 15:
        timestamp -= 1.
      if scenario == 5 and index == 10:
        timestamp -= .05
      shift = index // 4 % 4 if scenario in (1, 2, 6) else 0
      rgb = np.roll(base, shift, axis=1)
      if scenario == 2 and index in (8, 9, 16):
        rgb = np.zeros_like(rgb)
      if scenario == 7:
        rgb = np.full_like(rgb, 119)
      proposals = [dict(box=[30. + shift, 20., 48. + shift, 52.], score=.95)]
      if scenario == 6:
        proposals = [dict(box=[float(5 + n % 8 * 22), float(4 + n // 8 * 35), float(15 + n % 8 * 22), float(24 + n // 8 * 35)], score=.9) for n in range(24)]
      elif scenario in (0, 1, 2) and 10 <= index < 20:
        proposals = []
      elif scenario == 5:
        proposals += [dict(box=[34. + shift, 21., 52. + shift, 53.], score=.8)]
        if index % 2:
          proposals.reverse()
      network.proposals, network.calls, network.tick = proposals, [], index
      path = args.output / f'{scenario}-{index}.rgb'
      path.write_bytes(rgb.tobytes())
      frames.append(dict(path=str(path.resolve()), width=192, height=128, timestamp=timestamp, proposals=proposals, color=dict(label=('red', 'green', 'unknown', 'left,red')[index % 4], score=(.9, .8, .1, .95)[index % 4])))
      result = sequence.process(rgb, timestamp)
      state = {k: v for k, v in vars(sequence).items() if k != 'neural'}
      outputs.append(frozen(dict(result=result, state=state, calls=network.calls)))
    cases.append(frames)
    expected.append(outputs)
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  if args.binary:
    raw = subprocess.check_output([str(args.binary.resolve())], input=json.dumps(cases).encode())
    (args.output / 'actual.json').write_bytes(raw)
    (args.output / 'receipt.json').write_text(json.dumps(compare(json.loads(raw), expected)) + '\n')
  print(json.dumps(dict(scenarios=len(cases), frames=sum(map(len, cases)), native_compared=bool(args.binary), scope='scripted_network_actual_sequence_no_inference')))


if __name__ == '__main__':
  main()
