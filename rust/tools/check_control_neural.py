#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess

import numpy as np
from control_neural_source import evaluate
from control_neural_compare import compare_models


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--trace', type=Path, required=True)
  parser.add_argument('--numerics', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  root = Path('opendbc_repo/opendbc/car/torque_data').resolve()
  random = np.random.default_rng(150)
  inputs = [[10.0, 0.0, 0.2, 0.0], [0.0, 0.0, 0.0, 0.0]]
  for i in range(64):
    row = random.normal(0.0, 1.0, 18 if i % 2 else 4)
    row[0], row[3] = random.uniform(0.0, 40.0), random.uniform(-0.2, 0.2)
    inputs.append(row.tolist())
  exponents = np.concatenate(
    [random.uniform(-100.0, 88.0, 4096).astype(np.float32), np.array([-np.inf, -104.0, 0.0, 88.72284, np.inf, np.nan], dtype=np.float32)]
  )
  request = {'inputs': inputs, 'exp': exponents.view(np.uint32).tolist()}
  expected = evaluate(request, root)
  (args.evidence / 'input.json').write_text(json.dumps(request) + '\n')
  (args.evidence / 'source.json').write_text(json.dumps(expected, indent=2) + '\n')
  output = args.evidence / 'native.json'
  child = subprocess.run([args.trace.resolve(), args.numerics.resolve(), root, output.resolve()], input=json.dumps(request), text=True, capture_output=True)
  (args.evidence / 'process.log').write_text(child.stdout + child.stderr + f'\nEXIT {child.returncode}\n')
  child.check_returncode()
  actual = json.loads(output.read_text())
  result = compare_models(expected, actual)
  (args.evidence / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))


if __name__ == '__main__':
  main()
