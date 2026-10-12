"""Original threaded decode queue policy with a controlled decoder; no codec substitution claim."""

import argparse
import ast
from collections import deque
from dataclasses import dataclass, replace
import json
from pathlib import Path
import sys
import threading
import time


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  args = parser.parse_args()
  source = args.root / 'openpilot/selfdrive/carrot/cluster/cluster_navi_source.py'
  sys.path.insert(0, str(source.parent))
  from cluster_models import NaviMediaFrame

  names = {'DecodedH264Frame', '_H264DecodeRequest', '_H264DecodeResult', 'H264DecodeWorker'}
  nodes = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)]
  nodes += [
    node
    for node in ast.parse(source.read_text()).body
    if isinstance(node, ast.ClassDef)
    and node.name in names
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id == 'H264_DECODE_QUEUE_MAX' for target in node.targets)
  ]
  limits = args.root / 'openpilot/selfdrive/carrot/carrot_navi.py'
  nodes[1:1] = [
    node
    for node in ast.parse(limits.read_text()).body
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == 'MAP_HZ_MAX' for target in node.targets)
  ]
  scope = {'dataclass': dataclass, 'replace': replace, 'deque': deque, 'threading': threading, 'H264Decoder': object, 'NaviMediaFrame': NaviMediaFrame}
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(source), 'exec'), scope)
  entered, release = threading.Event(), threading.Event()
  trace = []

  class Decoder:
    def __init__(self):
      trace.append('create')

    def decode(self, packet, *, sequence, width, height):
      trace.append(f'decode:{sequence}:{width}:{height}:{list(packet)}')
      if sequence == 0:
        entered.set()
        assert release.wait(2)
      if sequence == 99:
        raise RuntimeError('controlled decode failure')
      return scope['DecodedH264Frame'](width, height)

    def reset(self):
      trace.append('reset')

    def close(self):
      trace.append('close')

  worker = scope['H264DecodeWorker'](decoder_factory=Decoder)

  def submit(key, sequence, epoch=0, keyframe=False, width=2):
    worker.submit(scope['_H264DecodeRequest'](epoch, key, sequence, bytes([sequence]), bytes([7]), 0, keyframe, width, 2))

  def wait(condition):
    end = time.monotonic() + 2
    while not condition():
      assert time.monotonic() < end
      time.sleep(0.001)

  def results(count):
    wait(lambda: len(worker._results) == count)
    return [[item.frame.key, item.frame.sequence, item.epoch] for item in worker.poll()]

  submit('a', 0)
  assert entered.wait(2)
  for sequence in range(1, 61):
    submit('a', sequence)
  submit('b', 7)
  submit('a', 61)
  submit('a', 62)
  assert worker.dropped_requests == 62
  submit('a', 80, keyframe=True)
  release.set()
  wait(lambda: any(item.startswith('decode:80:') for item in trace))
  initial = results(2)
  submit('a', 90, epoch=1, keyframe=True)
  epoch = results(1)
  submit('a', 91, epoch=1, keyframe=True, width=4)
  changed = results(1)
  submit('a', 99, epoch=1, width=4)
  wait(lambda: 'a' in worker._waiting_for_keyframe)
  submit('a', 100, epoch=1, width=4)
  submit('a', 101, epoch=1, keyframe=True, width=4)
  recovery = results(1)
  worker.close()
  worker.close()
  args.output.write_text(
    json.dumps({'initial': initial, 'epoch': epoch, 'changed': changed, 'recovery': recovery, 'dropped': worker.dropped_requests, 'trace': trace}, indent=2)
    + '\n'
  )


if __name__ == '__main__':
  main()
