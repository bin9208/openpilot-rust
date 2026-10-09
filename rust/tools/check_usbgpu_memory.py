"""Original page-table mutations and allocations versus native memory manager."""

from __future__ import annotations
import argparse
import ast
import collections
import dataclasses
import enum
import functools
import json
from pathlib import Path
import subprocess
import sys
import types
from typing import Any, ClassVar

ROOT = Path(__file__).resolve().parents[2]


class Vram:
  def __init__(self):
    self.entries, self.trace = {}, []

  def view(self, address, size, fmt):
    assert size == 4096 and fmt == 'Q'
    outer = self

    class Entries:
      def __getitem__(self, index):
        return outer.entries.get(address + index * 8, 0)

      def __setitem__(self, index, value):
        outer.entries[address + index * 8] = value
        outer.trace.append(['write', address + index * 8, value])

    return Entries()

  def __setitem__(self, sl, data):
    assert isinstance(sl, slice) and len(data) == sl.stop - sl.start and not any(data)
    self.entries = {address: value for address, value in self.entries.items() if not sl.start <= address < sl.stop}
    self.trace.append(['zero', sl.start, len(data)])


def extract(path, classes, module):
  tree = ast.parse(path.read_text())
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)] + [
    node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in classes
  ]
  ast.fix_missing_locations(tree)
  exec(compile(tree, str(path), 'exec'), module.__dict__)


def source(case):
  module = types.ModuleType('memory_source_reference')
  sys.modules[module.__name__] = module
  module.__dict__.update(
    collections=collections,
    dataclasses=dataclasses,
    enum=enum,
    functools=functools,
    Any=Any,
    ClassVar=ClassVar,
    round_up=lambda n, d: (n + d - 1) // d * d,
    getenv=lambda name, default=0: case['gmmu'] if name == 'GMMU' else default,
  )
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/autogen/am/am.py'
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for node in tree.body
    if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id.startswith(('AMDGPU_PTE_', 'AMDGPU_PDE_')) for target in node.targets)
  ]
  constants = {}
  exec(compile(tree, str(path), 'exec'), constants)
  am = types.SimpleNamespace(
    **{key: value for key, value in constants.items() if not key.startswith('__')},
    GC_HWIP=0,
    AMDGPU_VM_PTB=3,
    AMDGPU_VM_PDB0=2,
    AMDGPU_VM_PDB1=1,
    AMDGPU_VM_PDB2=0,
  )
  module.am = am
  extract(
    ROOT / 'tinygrad_repo/tinygrad/runtime/support/memory.py',
    {'TLSFAllocator', 'AddrSpace', 'VirtMapping', 'PageTableTraverseContext', 'MemoryManager'},
    module,
  )
  extract(ROOT / 'tinygrad_repo/tinygrad/runtime/support/am/amdev.py', {'AMPageTableEntry', 'AMMemoryManager'}, module)
  ip_tree = ast.parse((ROOT / 'tinygrad_repo/tinygrad/runtime/support/am/ip.py').read_text())
  ip_tree.body = [
    node
    for parent in ip_tree.body
    if isinstance(parent, ast.ClassDef) and parent.name == 'AM_GMC'
    for node in parent.body
    if isinstance(node, ast.FunctionDef) and node.name in {'get_pte_flags', 'is_pte_huge_page'}
  ]
  exec(compile(ip_tree, '<original GMC PTE methods>', 'exec'), module.__dict__)
  vram = Vram()
  device = types.SimpleNamespace(is_booting=True, smi_dev=False, vram=vram, paddr2xgmi=lambda value: value, xgmi2paddr=lambda value: value)

  class Gmc:
    address_space_mask = (1 << 44) - 1
    adev = types.SimpleNamespace(ip_ver={0: (case['gfx'], 0, 0)}, soc=types.SimpleNamespace(module=types.SimpleNamespace(MTYPE_UC=3)))

    def get_pte_flags(self, *args, **kwargs):
      return module.get_pte_flags.__wrapped__(self, *args, **kwargs)

    def is_pte_huge_page(self, *args):
      return module.is_pte_huge_page(self, *args)

    def flush_tlb(self, ip, vmid):
      assert vmid == 0
      if ip == 'MM':
        vram.trace.append(['flush'])

  device.gmc = Gmc()
  memory = module.AMMemoryManager(
    device,
    512 << 20,
    32 << 20,
    module.AMPageTableEntry,
    48,
    [12, 21, 30, 39],
    0x200000000000,
    [(1 << (power + 12), (2 << 20) if power >= 9 else 4096) for power in range(27, -1, -1)],
    reserve_ptable=case['reserve'],
  )
  device.mm = memory
  device.is_booting = False
  allocated, results = {}, []

  def view(mapping):
    return {
      'address': mapping.va_addr,
      'size': mapping.size,
      'physical': [list(pair) for pair in mapping.paddrs],
      'uncached': mapping.uncached,
      'snooped': mapping.snooped,
    }

  for operation in case['operations']:
    identity = operation['id']
    match operation['kind']:
      case 'alloc':
        mapping = memory.valloc(operation['size'], operation['align'], operation['uncached'], operation['contiguous'])
        allocated[identity] = mapping
        results.append(view(mapping))
      case 'free':
        memory.vfree(allocated.pop(identity))
        results.append(None)
      case 'map_system':
        mapping = memory.map_range(
          memory.alloc_vaddr(8192, 4096), 8192, [(0x200000, 4096), (0x201000, 4096)], module.AddrSpace.SYS, uncached=True, snooped=True
        )
        allocated[identity] = mapping
        results.append(view(mapping))
      case 'unmap':
        mapping = allocated.pop(identity)
        memory.unmap_range(mapping.va_addr, mapping.size)
        results.append(None)
  return {'results': results, 'writes': vram.trace}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  cases = []
  for gfx in [9, 10, 12]:
    for reserve in [False, True]:
      for gmmu in [False, True]:
        operations = [
          {'kind': 'alloc', 'id': i, 'size': size, 'align': 4096, 'uncached': i % 2 == 0, 'contiguous': i % 3 == 0}
          for i, size in enumerate([4096, 8192, 2 << 20, 6 << 20, 8 << 20, 3 << 20])
        ]
        operations += [{'kind': 'free', 'id': i} for i in [1, 3, 0, 2, 5, 4]]
        operations += [{'kind': 'alloc', 'id': 9, 'size': 32 << 20, 'align': 2 << 20, 'uncached': False, 'contiguous': False}, {'kind': 'free', 'id': 9}]
        operations += [{'kind': 'map_system', 'id': 10}, {'kind': 'unmap', 'id': 10}]
        cases.append({'gfx': gfx, 'reserve': reserve, 'gmmu': gmmu, 'operations': operations})
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), capture_output=True, text=True, timeout=30)
  assert process.returncode == 0, process.stderr
  results = []
  for case, line in zip(cases, process.stdout.splitlines(), strict=True):
    native, original = json.loads(line), source(case)
    results.append({'input': case, 'source': original, 'native': native, 'passed': original == native})
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': results}, indent=2) + '\n')
  assert all(result['passed'] for result in results), [index for index, result in enumerate(results) if not result['passed']]
  print(f'PASS {len(results)} original AMD mapping/free write traces (gfx9/10/12, reserved tables, GMMU on/off)')


if __name__ == '__main__':
  main()
