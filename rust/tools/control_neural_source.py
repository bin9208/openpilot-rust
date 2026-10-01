"""Execute the unchanged FluxModel/NanoFFModel classes with the active NumPy build."""

import ast
import json
from pathlib import Path
import sys

import numpy as np


def models():
  path = Path(__file__).resolve().parents[2] / 'opendbc_repo/opendbc/car/interfaces.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in ('FluxModel', 'NanoFFModel')]
  scope = {'np': np, 'json': json}
  exec(compile(tree, str(path), 'exec'), scope)
  return scope['FluxModel'], scope['NanoFFModel']


def evaluate(request, root):
  flux, nano = models()
  inputs = request['inputs']
  exponents = np.array(request['exp'], dtype=np.uint32).view(np.float32)
  expected = {'flux': {}, 'nano': {}, 'exp': np.exp(exponents).view(np.uint32).tolist()}
  for path in sorted((root / 'lat_models').glob('*.json')):
    model = flux(path)
    expected['flux'][path.name] = {'values': [model.evaluate(value) for value in inputs], 'friction': bool(model.friction_override)}
  for name in json.loads((root / 'neural_ff_weights.json').read_text()):
    model = nano(root / 'neural_ff_weights.json', name)
    expected['nano'][name] = [model.predict(value[:4]) for value in inputs]
  return expected


if __name__ == '__main__':
  request = json.loads(Path(sys.argv[1]).read_text())
  Path(sys.argv[2]).write_text(json.dumps(evaluate(request, Path(sys.argv[3])), indent=2) + '\n')
  print(json.dumps({'numpy': np.__version__, 'architecture': __import__('platform').machine(), 'source': str(Path(__file__).resolve())}))
