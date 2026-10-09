"""Original USB compute/SDMA/AQL ring submissions versus native owned ring I/O."""

from __future__ import annotations
import argparse
import array
import ast
import importlib
import json
from pathlib import Path
import random
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))


def source(case):
  events = []

  class View:
    def __init__(self, size, kind='ring', offset=0, width=4):
      self.nbytes, self.kind, self.offset, self.width, self.addr = size, kind, offset, width, 0x210001000000

    def __len__(self):
      return self.nbytes // self.width

    def __setitem__(self, index, value):
      if self.kind != 'ring':
        events.append([self.kind, value])
      elif isinstance(index, slice):
        data = list(bytes(value))
        offset = self.offset + (index.start or 0) * self.width
        assert offset + len(data) <= case['size'], 'fixture write outside ring'
        events.append(['bytes', offset, data])
      else:
        events.append(['word', self.offset + index * self.width, value])

    def view(self, offset=0, size=None, fmt='I'):
      return View(self.nbytes if size is None else size, self.kind, self.offset + offset, 1 if fmt == 'B' else 4)

  path = ROOT / 'tinygrad_repo/tinygrad/runtime/ops_amd.py'
  tree = ast.parse(path.read_text())
  methods = {}
  for cls in tree.body:
    if not isinstance(cls, ast.ClassDef):
      continue
    for node in cls.body:
      if isinstance(node, ast.FunctionDef) and node.name in {'_submit', 'signal_doorbell'}:
        methods[(cls.name, node.name)] = node
  env = {
    'array': array,
    'data64_le': lambda v: [v & 0xFFFFFFFF, v >> 32],
    'System': types.SimpleNamespace(memory_barrier=lambda: events.append(['barrier'])),
    'hsa': importlib.import_module('tinygrad.runtime.autogen.hsa'),
  }
  selected = methods[({'compute': 'AMDComputeQueue', 'copy': 'AMDCopyQueue', 'aql': 'AMDComputeAQLQueue'}[case['kind']], '_submit')]
  module = ast.Module(
    body=[ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0), methods[('AMDQueueDesc', 'signal_doorbell')], selected],
    type_ignores=[],
  )
  ast.fix_missing_locations(module)
  exec(compile(module, str(path), 'exec'), env)
  desc = types.SimpleNamespace(ring=View(case['size']), write_ptr=View(8, 'pointer'), doorbell=View(8, 'doorbell'), put_value=case['put'])
  desc.signal_doorbell = lambda dev, **kwargs: env['signal_doorbell'](desc, dev, **kwargs)
  dev = types.SimpleNamespace(compute_queue=desc, sdma_queue=lambda idx: desc, is_am=lambda: True, is_usb=lambda: True)
  words = case['words']
  queue = types.SimpleNamespace(
    dev=types.SimpleNamespace(xccs=case['xccs']),
    binded_device=dev if case['bound'] else None,
    _q=words,
    internal_cmd_sizes=case['sizes'],
    indirect_cmd=words,
    queue_idx=0,
    pm4=importlib.import_module('tinygrad.runtime.autogen.am.pm4_nv'),
  )
  if case['kind'] == 'aql':
    packets = [array.array('I', words[index : index + 16]).tobytes() for index in range(0, len(words), 16)]
    queue._cmds = packets
    assert case['bound']
  failed = False
  try:
    env['_submit'](queue, dev)
  except (AssertionError, ValueError):
    failed = True
  return {'events': events, 'put': desc.put_value, 'failed': failed}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  rng, cases = random.Random(154), []
  for kind in ['compute', 'copy', 'aql']:
    for size in [256, 512, 4096]:
      for offset in [0, 4, 32, 64, size - 64, size - 4]:
        for bound in [False, True]:
          if kind == 'aql' and not bound:
            continue
          for xccs in [1, 2] if kind == 'compute' else [1]:
            sizes = [7, 6, 4, 3] if kind == 'copy' else [32]
            if kind == 'copy' and bound:
              sizes = [6]
            put = offset if kind == 'copy' else offset // (64 if kind == 'aql' else 4)
            words = [rng.randrange(1 << 32) for _ in range(sum(sizes))]
            cases.append({'kind': kind, 'size': size, 'put': put, 'bound': bound, 'xccs': xccs, 'words': words, 'sizes': sizes})
  expected = [source(case) for case in cases]
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  rows = [{'case': case, 'source': old, 'native': new, 'passed': old == new} for case, old, new in zip(cases, expected, actual, strict=True)]
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': rows}, indent=2) + '\n')
  assert all(row['passed'] for row in rows), [index for index, row in enumerate(rows) if not row['passed']]
  print(f'PASS {len(rows)} USB ring submissions including wrap, indirect alignment, pointer and doorbell order')


if __name__ == '__main__':
  main()
