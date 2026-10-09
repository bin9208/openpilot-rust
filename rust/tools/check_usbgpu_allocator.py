"""Allocation/free address parity against the unchanged source TLSF allocator."""

from __future__ import annotations
import argparse
import ast
import collections
import functools
import json
from pathlib import Path
import random
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def source(case):
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/support/memory.py'
  tree = ast.parse(path.read_text())
  tree.body = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'TLSFAllocator']
  env = {'collections': collections, 'functools': functools, 'round_up': lambda n, d: (n + d - 1) // d * d}
  exec(compile(tree, str(path), 'exec'), env)
  allocator = env['TLSFAllocator'](case['size'], case['base'])
  allocated, results = {}, []
  for operation in case['operations']:
    identity = operation['id']
    if operation['kind'] == 'alloc':
      try:
        address = allocator.alloc(operation['size'], operation['align'])
        allocated[identity] = address
        results.append(address)
      except MemoryError:
        results.append(None)
    elif identity in allocated:
      allocator.free(allocated.pop(identity))
      results.append(True)
    else:
      results.append(False)
  return results


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  cases = []
  for seed in range(20):
    generator = random.Random(seed)
    identities, operations = [], []
    for identity in range(300):
      if identities and generator.random() < 0.4:
        old = generator.choice(identities)
        identities.remove(old)
        operations.append({'kind': 'free', 'id': old})
      else:
        identities.append(identity)
        operations.append(
          {
            'kind': 'alloc',
            'id': identity,
            'size': generator.choice([0, 1, 15, 16, 17, 4095, 4096, 4097, 65536, 2 << 20]),
            'align': generator.choice([1, 16, 4096, 2 << 20]),
          }
        )
    operations.extend({'kind': 'free', 'id': identity} for identity in identities)
    operations.append({'kind': 'alloc', 'id': 301, 'size': 16 << 20, 'align': 1})
    cases.append({'size': 16 << 20, 'base': (1 << 40) if seed % 2 else 0, 'operations': operations})
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  results = []
  for case, line in zip(cases, process.stdout.splitlines(), strict=True):
    native = json.loads(line)
    original = source(case)
    results.append({'input': case, 'source': original, 'native': native, 'passed': original == native})
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': results}, indent=2) + '\n')
  assert all(result['passed'] for result in results)
  print(f'PASS {sum(len(case["operations"]) for case in cases)} TLSF allocation/free operations in {len(cases)} deterministic scenarios')


if __name__ == '__main__':
  main()
