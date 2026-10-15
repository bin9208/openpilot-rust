"""Actual pinned NeuralSignal CPU/ORT tensors and inference; temporary source oracle."""
import argparse
import hashlib
import json
from pathlib import Path
import cv2
import numpy as np
from signal_source import classes


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--models', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  cv2.setNumThreads(1)
  cv2.ocl.setUseOpenCL(False)
  scope = classes('tools/signal_analysis/neural_signal.py', ['NeuralSignal'])
  scope.update(np=np, cv2=cv2, Path=Path, hashlib=hashlib)
  scope['overlap'] = classes('tools/signal_analysis/signal_tracker.py', ['overlap'])['overlap']
  network = scope['NeuralSignal'](args.models / 'detector.onnx', args.models / 'classifier.onnx', backend='cpu')
  tensors = []
  class Capture:
    def __init__(self, session, name):
      self.session, self.name = session, name
    def run(self, outputs, values):
      index = len(tensors)
      input_name, tensor = next(iter(values.items()))
      result = self.session.run(outputs, values)
      inp = args.output / f'{index}-input.f32'
      inp.write_bytes(tensor.tobytes())
      record = dict(network=self.name, input_name=input_name, input_path=str(inp.resolve()), input_shape=list(tensor.shape), input_sha256=hashlib.sha256(tensor.tobytes()).hexdigest(), outputs=[])
      for n, output in enumerate(result):
        path = args.output / f'{index}-output-{n}.f32'
        path.write_bytes(output.tobytes())
        record['outputs'].append(dict(path=str(path.resolve()), shape=list(output.shape), sha256=hashlib.sha256(output.tobytes()).hexdigest()))
      tensors.append(record)
      return result
  network.detector = Capture(network.detector, 'detector')
  network.classifier = Capture(network.classifier, 'classifier')
  cases = []
  for index in range(4):
    rng = np.random.default_rng(473 + index)
    rgb = rng.integers(0, 256, (760, 1344, 3), np.uint8) if index == 0 else np.full((760, 1344, 3), (60, 65, 70), np.uint8)
    if index:
      rgb[140:220, 625:645] = (10, 10, 10)
      rgb[145:160, 628:642] = (255, 0, 0) if index < 3 else (0, 255, 0)
      rgb[200:215, 628:642] = (0, 255, 0) if index == 2 else (10, 10, 10)
    path = args.output / f'image-{index}.rgb'
    path.write_bytes(rgb.tobytes())
    rect = (268, 30, 1142, 456) if index % 2 else None
    bounds = [625., 140., 645., 220.]
    proposals = network.detect(rgb, rect)
    color = network.classify(rgb, bounds)
    cases.append(dict(path=str(path.resolve()), width=1344, height=760, rect=rect, box=bounds, proposals=proposals, color=color))
  import onnxruntime as ort
  provider_path = next((Path(ort.__file__).parent / 'capi').glob('onnxruntime_pybind11_state*.so'))
  receipt = dict(reference_provider_sha256=hashlib.sha256(provider_path.read_bytes()).hexdigest(), source_commit='e8e152dbcfd9574a1791ed12026d1880e87689b3', onnxruntime=ort.__version__, provider='CPUExecutionProvider', threads=1, frames=len(cases), tensors=tensors, cases=cases, native_compared=False)
  (args.output / 'expected.json').write_text(json.dumps(receipt))
  print(json.dumps(dict(frames=len(cases), runs=len(tensors), proposals=sum(len(c['proposals']) for c in cases), labels=[c['color']['label'] for c in cases], scope='actual_source_cpu_ort_no_native_comparison')))


if __name__ == '__main__':
  main()
