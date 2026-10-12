import argparse
import ast
from dataclasses import dataclass
import hashlib
import itertools
import json
from pathlib import Path
import queue
import random
import subprocess
import threading


def original(path):
  tree = ast.parse(path.read_text())
  nodes = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)]
  for node in tree.body:
    if isinstance(node, ast.ClassDef) and node.name == '_NativeAccessUnit':
      nodes.append(node)
    elif isinstance(node, ast.ClassDef) and node.name == 'H264UsbPipeline':
      node.body = [method for method in node.body if isinstance(method, ast.FunctionDef) and method.name == '_enqueue_native_access_unit']
      nodes.append(node)
  scope = {'dataclass': dataclass, 'queue': queue, 'NATIVE_ACCESS_UNIT_QUEUE_MAX': 3}
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
  return scope


def unit(index, flags, empty=False):
  return {'chunks': [] if empty else [[index % 3], [index % 3, index % 5]], 'codec_config': bool(flags & 1), 'keyframe': bool(flags & 2)}


def expected(scope, cases):
  rows = []
  for case in cases:
    owner = scope['H264UsbPipeline'].__new__(scope['H264UsbPipeline'])
    owner._packet_queue = queue.Queue(maxsize=3)
    owner._packet_queue_lock = threading.Lock()
    dropped = [0, 0]
    owner._record_h264_drop = lambda units, chunks, counts=dropped: counts.__setitem__(slice(None), [units, chunks])
    owner._record_h264_queue_depth = lambda depth: None
    steps = []
    for incoming in case['steps']:
      dropped[:] = [0, 0]
      owner._enqueue_native_access_unit([bytes(chunk) for chunk in incoming['chunks']], codec_config=incoming['codec_config'], keyframe=incoming['keyframe'])
      kept = [
        {'chunks': [list(chunk) for chunk in item.chunks], 'codec_config': item.codec_config, 'keyframe': item.keyframe} for item in owner._packet_queue.queue
      ]
      steps.append({'kept': kept, 'dropped_units': dropped[0], 'dropped_chunks': dropped[1]})
    rows.append(steps)
  return rows


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  source = args.root.resolve() / 'openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py'
  cases = [{'steps': [unit(index, flag) for index, flag in enumerate(flags)]} for length in range(1, 7) for flags in itertools.product(range(4), repeat=length)]
  rng = random.Random(253)
  cases += [{'steps': [unit(index, rng.randrange(4), rng.randrange(8) == 0) for index in range(120)]} for _ in range(128)]
  source_rows = expected(original(source), cases)
  (output / 'input.json').write_text(json.dumps(cases))
  (output / 'source.json').write_text(json.dumps(source_rows))
  result = {'cases': len(cases), 'steps': sum(len(case['steps']) for case in cases), 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest()}
  if args.binary:
    binary = args.binary.resolve()
    invocation = [str(binary), str(output / 'native.json')]
    process = subprocess.run(invocation, input=json.dumps(cases), text=True, capture_output=True, timeout=30, check=True)
    (output / 'native.stdout').write_text(process.stdout)
    (output / 'native.stderr').write_text(process.stderr)
    actual = json.loads((output / 'native.json').read_text())
    errors = [{'case': index, 'source': left, 'native': right} for index, (left, right) in enumerate(zip(source_rows, actual, strict=True)) if left != right]
    result.update(
      {'invocation': invocation, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'matched': len(cases) - len(errors), 'errors': errors}
    )
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({key: value for key, value in result.items() if key != 'errors'}))
  if result.get('errors'):
    raise SystemExit(1)


if __name__ == '__main__':
  main()
