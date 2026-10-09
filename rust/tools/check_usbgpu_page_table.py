"""Compare AMD PTE flags and fragment calculation against original methods."""

from __future__ import annotations
import argparse
import ast
import functools
import itertools
import json
from pathlib import Path
import subprocess
import types

ROOT = Path(__file__).resolve().parents[2]


def methods(path, classname, names, env):
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for group in tree.body
    if isinstance(group, ast.ClassDef) and group.name == classname
    for node in group.body
    if isinstance(node, ast.FunctionDef) and node.name in names
  ]
  exec(compile(tree, str(path), 'exec'), env)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  constants = {}
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/autogen/am/am.py'
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for node in tree.body
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id.startswith(('AMDGPU_PTE_', 'AMDGPU_PDE_')) for target in node.targets)
  ]
  exec(compile(tree, str(path), 'exec'), constants)
  am = types.SimpleNamespace(
    **{key: value for key, value in constants.items() if not key.startswith('__')}, GC_HWIP=0, AMDGPU_VM_PTB=3, AMDGPU_VM_PDB0=2, AMDGPU_VM_PDB1=1
  )
  env = {'am': am, 'functools': functools}
  methods(ROOT / 'tinygrad_repo/tinygrad/runtime/support/am/ip.py', 'AM_GMC', {'get_pte_flags', 'is_pte_huge_page'}, env)
  methods(ROOT / 'tinygrad_repo/tinygrad/runtime/support/memory.py', 'MemoryManager', {'_frag_size'}, env)
  inputs, expected = [], []
  for gfx, level, table, uncached, system, snooped, valid, frag in itertools.product(
    [9, 10, 11, 12], range(4), [False, True], [False, True], [False, True], [False, True], [False, True], [0, 1, 9, 31]
  ):
    context = types.SimpleNamespace(adev=types.SimpleNamespace(ip_ver={0: (gfx, 0, 0)}, soc=types.SimpleNamespace(module=types.SimpleNamespace(MTYPE_UC=3))))
    flags = env['get_pte_flags'].__wrapped__(context, level, table, frag, uncached, system, snooped, valid)
    page = level == 3 or bool(env['is_pte_huge_page'](context, level, flags))
    inputs.append(
      {
        'kind': 'flags',
        'gfx': gfx,
        'level': level,
        'table': table,
        'uncached': uncached,
        'system': system,
        'snooped': snooped,
        'valid': valid,
        'fragment': frag,
      }
    )
    expected.append({'flags': flags, 'page': page})
  for address, size, must_cover in itertools.product(
    [0, 0x1000, 0x3000, 0x200000, 0x200000000000], [0x1000, 0x3000, 0x200000, 0x300000, 1 << 30], [False, True]
  ):
    inputs.append({'kind': 'fragment', 'address': address, 'size': size, 'must_cover': must_cover})
    expected.append(env['_frag_size'](None, address, size, must_cover))
  process = subprocess.run([args.binary], input=''.join(json.dumps(value) + '\n' for value in inputs), capture_output=True, text=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  results = [
    {'input': value, 'source': original, 'native': native, 'passed': original == native}
    for value, original, native in zip(inputs, expected, actual, strict=True)
  ]
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': results}, indent=2) + '\n')
  assert all(result['passed'] for result in results)
  print(f'PASS {len(results)} AMD PTE flag/page/fragment comparisons')


if __name__ == '__main__':
  main()
