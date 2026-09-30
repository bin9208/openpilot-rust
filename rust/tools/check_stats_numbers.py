# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
# Run: python rust/tools/check_stats_numbers.py --binary rust/target/debug/examples/stats_trace --output PATH
"""Compare numeric aggregation with statements compiled from unchanged statsd.py."""
import argparse
import ast
import hashlib
import json
import random
import struct
import subprocess
from pathlib import Path
from collections import defaultdict
from datetime import datetime, UTC


def source_oracle():
  source = Path(__file__).resolve().parents[2] / 'openpilot/system/statsd.py'
  tree = ast.parse(source.read_text())
  main = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  line = next(node for node in main.body if isinstance(node, ast.FunctionDef))
  loop = next(node for node in ast.walk(main) if isinstance(node, ast.If) and isinstance(node.test, ast.BoolOp))
  gauge = next(node for node in loop.body if isinstance(node, ast.For))
  sample = [node for node in loop.body if isinstance(node, ast.For)][1]
  code = compile(ast.fix_missing_locations(ast.Module(body=[line, gauge, sample], type_ignores=[])), str(source), 'exec')
  return source, code


def cases():
  yield [0.0, -0.0]
  yield [1e16, 1.0, -1e16]
  yield [1e308, 1e308, float('-inf')]
  rng = random.Random(75)
  for size in [1, 2, 3, 11, 21, 31, 32, 33, 63, 64, 65, 127, 128, 129, 511, 1024, 4096]:
    for trial in range(35):
      values = [rng.uniform(-1e8, 1e8) for _ in range(size)]
      if trial % 3 == 0:
        values = [struct.unpack('d', rng.getrandbits(64).to_bytes(8, 'little'))[0] for _ in values]
      if trial % 3 == 1:
        for _ in range(size // 8 + 1):
          values[rng.randrange(size)] = rng.choice([float('nan'), float('inf'), float('-inf'), -0.0])
      if trial % 5 == 0:
        values.reverse()
      yield values


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--runner', type=Path)
  args = parser.parse_args()
  source, code = source_oracle()
  frames = []
  expected = []
  for values in cases():
    frames.append(json.dumps({'metrics': [f'x:{value}|sa' for value in values]}))
    scope = {'datetime': datetime, 'dongle_id': 'test', 'gauges': {}, 'samples': defaultdict(list, x=values),
             'result': '', 'current_time': datetime.fromtimestamp(0, UTC),
             'tags': dict(started=False, version='v', branch='test', dirty=False,
                          origin='github.com/test/openpilot', deviceType='pc')}
    exec(code, scope)
    expected.append(scope['result'].replace('"test" 0\n', '"test" 123\n'))
  run = subprocess.run(([str(args.runner)] if args.runner else []) + [args.binary], input='\n'.join(frames)+'\n', capture_output=True, text=True, check=True)
  actual = [json.loads(line) for line in run.stdout.splitlines()]
  differences = [{'case': index, 'expected': want, 'actual': got, 'input': json.loads(frames[index])}
                 for index, (want, got) in enumerate(zip(expected, actual, strict=True)) if want != got]
  args.output.write_text(json.dumps({'cases': len(frames), 'differences': differences,
      'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
      'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}, indent=2))
  print(f'{len(frames)} aggregation cases, {len(differences)} differences')
  assert not differences


if __name__ == '__main__':
  main()
