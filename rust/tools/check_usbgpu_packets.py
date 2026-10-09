"""Compare native command words against unchanged source queue methods."""

from __future__ import annotations
import argparse
import ast
import contextlib
import importlib
import json
from pathlib import Path
import random
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))


class Queue:
  def __init__(self):
    self._q, self.binded_device = [], None

  def q(self, *values):
    self._q.extend(values)


def source(case):
  pm4 = importlib.import_module('tinygrad.runtime.autogen.am.pm4_' + ('soc15' if case['gfx'] == 9 else 'nv'))
  sdma = importlib.import_module(f'tinygrad.runtime.autogen.am.sdma_{case.get("sdma", 6)}_0_0')
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/ops_amd.py'
  tree = ast.parse(path.read_text())
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)] + [
    node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in {'AMDComputeQueue', 'AMDCopyQueue'}
  ]
  ast.fix_missing_locations(tree)
  env = {
    'HWQueue': Queue,
    'contextlib': contextlib,
    'data64_le': lambda v: [v & 0xFFFFFFFF, v >> 32],
    'lo32': lambda v: v & 0xFFFFFFFF,
    'hi32': lambda v: v >> 32,
    'WAIT_REG_MEM_FUNCTION_GEQ': 5,
    'WAIT_REG_MEM_FUNCTION_EQ': 3,
  }
  exec(compile(tree, str(path), 'exec'), env)
  dev = types.SimpleNamespace(pm4=pm4, sdma=sdma, soc=None, gc=None, nbio=None, target=(case['gfx'], 0, 0), xccs=case['xccs'], is_am=lambda: True)
  queue = env['AMDComputeQueue'](dev) if case['kind'] == 'compute' else env['AMDCopyQueue'](dev, case['max_copy'])
  for op in case['ops']:
    method, address, value = op['op'], op.get('address', 0), op.get('value', 0)
    buffer = types.SimpleNamespace(va_addr=address)
    signal = types.SimpleNamespace(value_addr=address, timestamp_addr=address, owner=dev if op.get('owned', True) else None, is_timeline=True)
    if method == 'copy':
      queue.copy(buffer, types.SimpleNamespace(va_addr=op['source']), op['size'])
    elif method == 'wait' and case['kind'] == 'compute':
      queue.wait_reg_mem(value=value, mask=op['mask'], mem=address, op=op['operation'])
    elif method == 'wait':
      queue.wait(signal, value)
    elif method == 'acquire':
      queue.acquire_mem(addr=address, sz=op['size'], **{name: op[name] for name in ['gli', 'glm', 'glk', 'glv', 'gl1', 'gl2']})
    elif method == 'release':
      queue.release_mem(address=address, value=value, data_sel=op['data'], int_sel=op['interrupt'], ctxid=op['context'], cache_flush=op['flush'])
    elif method == 'signal':
      queue.signal(signal, value)
    elif method == 'timestamp':
      queue.timestamp(signal)
    elif method == 'write':
      queue.write(buffer, value, op['wide'])
  if case['kind'] == 'compute':
    return {'words': queue._q, 'indirect': [pm4.PACKET3(pm4.PACKET3_INDIRECT_BUFFER, 2), 0x1000000, 0x2100, len(queue._q) | pm4.INDIRECT_BUFFER_VALID]}
  padded = queue._q + [0] * (-len(queue._q) % 8)
  return {
    'words': queue._q,
    'sizes': queue.internal_cmd_sizes,
    'padded': padded,
    'indirect': [sdma.SDMA_OP_INDIRECT | sdma.SDMA_PKT_INDIRECT_HEADER_VMID(0), 0x2000000, 0x2100, len(padded), 0, 0],
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  rng, cases = random.Random(154), []
  for gfx in [9, 11, 12]:
    for xccs in [1, 2]:
      for kind in ['compute', 'copy']:
        for _iteration in range(12):
          ops = []
          for _ in range(10):
            common = {'address': rng.randrange(1 << 44) & ~7, 'value': rng.randrange(1 << 32)}
            method = rng.choice(['signal', 'timestamp', 'wait', 'write'] + (['copy'] if kind == 'copy' else ['release', 'acquire']))
            op = {'op': method, **common}
            if method == 'signal':
              op['owned'] = bool(rng.randrange(2))
            if method == 'wait':
              op.update(mask=rng.randrange(1 << 32), operation=rng.choice([3, 4, 5]))
            if method == 'write':
              op.update(wide=bool(rng.randrange(2)), value=rng.randrange(1 << 64))
            if method == 'copy':
              op.update(source=rng.randrange(1 << 44), size=rng.choice([0, 1, 4096, 16385]))
            if method == 'release':
              op.update(data=rng.randrange(4), interrupt=rng.randrange(4), context=rng.randrange(1 << 32), flush=bool(rng.randrange(2)))
            if method == 'acquire':
              op.update(size=rng.randrange(1 << 64), **{name: rng.randrange(2) for name in ['gli', 'glm', 'glk', 'glv', 'gl1', 'gl2']})
            ops.append(op)
          cases.append({'kind': kind, 'gfx': gfx, 'xccs': xccs, 'sdma': {9: 4, 11: 5, 12: 6}[gfx], 'max_copy': 4096, 'ops': ops})
  expected = [source(case) for case in cases]
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  rows = [{'case': case, 'source': old, 'native': new, 'passed': old == new} for case, old, new in zip(cases, expected, actual, strict=True)]
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': rows}, indent=2) + '\n')
  assert all(row['passed'] for row in rows), [index for index, row in enumerate(rows) if not row['passed']]
  print(f'PASS {len(rows)} source command sequences across gfx9/11/12, SDMA4/5/6 and one/two XCCs')


if __name__ == '__main__':
  main()
