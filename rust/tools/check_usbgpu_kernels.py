"""Original AMD program descriptors, scratch and dispatch versus native Rust."""

from __future__ import annotations
import argparse
import ast
import contextlib
import ctypes
import importlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
from tinygrad.runtime.autogen import hsa, amdgpu_kd
from tinygrad.runtime.support.amd import AMDIP, import_soc
from tinygrad.runtime.support.elf import elf_loader


class Queue:
  def __init__(self):
    self._q, self.binded_device = [], None

  def q(self, *values):
    self._q.extend(values)

  def bind_args_state(self, args):
    pass

  def bind_sints_to_mem(self, *values, mem, fmt='I', offset=0):
    data = struct.pack('<' + fmt * len(values), *values)
    ctypes.memmove(mem.addr + offset, data, len(data))

  def bind_sints(self, *values, mem, struct_t, start_field, fmt):
    self.bind_sints_to_mem(*values, mem=mem, fmt=fmt, offset=getattr(struct_t, start_field).offset)


class Program:
  def __init__(self, *args, **kwargs):
    pass

  def _fini(self, *args):
    pass


class ArgBuffer:
  def __init__(self, address):
    self.va_addr = address

  def offset(self, offset):
    return ArgBuffer(self.va_addr + offset)

  def cpu_view(self):
    return types.SimpleNamespace(addr=self.va_addr)


def original(case):
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/ops_amd.py'
  tree = ast.parse(path.read_text())
  selected = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in {'AMDProgram', 'AMDComputeQueue', 'AMDComputeAQLQueue'}]
  scratch = next(
    node
    for cls in tree.body
    if isinstance(cls, ast.ClassDef) and cls.name == 'AMDDevice'
    for node in cls.body
    if isinstance(node, ast.FunctionDef) and node.name == '_ensure_has_local_memory'
  )
  module = ast.Module(body=[ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0), *selected, scratch], type_ignores=[])
  ast.fix_missing_locations(module)
  env = {
    'HWQueue': Queue,
    'HCQProgram': Program,
    'CLikeArgsState': object,
    'BufferSpec': types.SimpleNamespace,
    'elf_loader': elf_loader,
    'ctypes': ctypes,
    'struct': struct,
    'hsa': hsa,
    'amdgpu_kd': amdgpu_kd,
    'weakref': types.SimpleNamespace(finalize=lambda *args: None),
    'contextlib': contextlib,
    'round_up': lambda n, d: (n + d - 1) // d * d,
    'ceildiv': lambda n, d: (n + d - 1) // d,
    'data64_le': lambda v: [v & 0xFFFFFFFF, v >> 32],
    'lo32': lambda v: v & 0xFFFFFFFF,
    'hi32': lambda v: v >> 32,
    'getenv': lambda key, default=0: default,
    'WAIT_REG_MEM_FUNCTION_GEQ': 5,
    'WAIT_REG_MEM_FUNCTION_EQ': 3,
    'EVENT_INDEX_PARTIAL_FLUSH': 4,
    'cast': lambda typ, value: value,
    'MMIOInterface': lambda addr, nbytes: types.SimpleNamespace(addr=addr),
    'AQL_HDR': (1 << hsa.HSA_PACKET_HEADER_BARRIER)
    | (hsa.HSA_FENCE_SCOPE_SYSTEM << hsa.HSA_PACKET_HEADER_SCACQUIRE_FENCE_SCOPE)
    | (hsa.HSA_FENCE_SCOPE_SYSTEM << hsa.HSA_PACKET_HEADER_SCRELEASE_FENCE_SCOPE),
  }
  exec(compile(module, str(path), 'exec'), env)
  gfx, xccs = case['gfx'], case['xccs']
  offsets = importlib.import_module('tinygrad.runtime.autogen.am.' + ('vega_offsets' if gfx == 9 else 'navi_offsets'))
  gc_version, nb_version = {9: ((9, 4, 3), (7, 4, 0)), 11: ((11, 0, 0), (4, 3, 0)), 12: ((12, 0, 0), (6, 3, 1))}[gfx]
  gc = AMDIP('gc', gc_version, {i: tuple(getattr(offsets, f'GC_BASE__INST{i}_SEG{s}', 0) for s in range(6)) for i in range(6)})
  nbio = AMDIP('nbio' if gfx < 12 else 'nbif', nb_version, {i: tuple(getattr(offsets, f'NBIO_BASE__INST{i}_SEG{s}', 0) for s in range(9)) for i in range(6)})
  pm4 = importlib.import_module('tinygrad.runtime.autogen.am.pm4_' + ('soc15' if gfx == 9 else 'nv'))
  dev = types.SimpleNamespace(
    target=(gfx, 0, 0),
    cu_cnt=16,
    se_cnt=2,
    xccs=xccs,
    max_private_segment_size=0,
    iface=types.SimpleNamespace(props={'max_slots_scratch_cu': 32, 'lds_size_in_kb': 64}),
    sqtt_enabled=False,
    gc=gc,
    nbio=nbio,
    pm4=pm4,
    soc=import_soc((gfx, 0, 0)),
    allocator=types.SimpleNamespace(alloc=lambda size, spec: types.SimpleNamespace(va_addr=0x210010000000, size=size), _copyin=lambda *args: None),
    synchronize=lambda: None,
    aql_desc=hsa.amd_queue_t(),
    aql_gart=types.SimpleNamespace(cpu_view=lambda: bytearray(256)),
  )
  dev._realloc = lambda old, size: (types.SimpleNamespace(va_addr=0x210001000000, size=size), True)
  dev._ensure_has_local_memory = lambda size: env['_ensure_has_local_memory'](dev, size)
  dev._ensure_has_local_memory(case['private'])
  prg = env['AMDProgram'](dev, 'fixture', Path(case['path']).read_bytes())
  expected = {
    'descriptor': {
      'group_segment_size': prg.group_segment_size,
      'private_segment_size': prg.private_segment_size,
      'kernargs_segment_size': prg.kernargs_segment_size,
      'wave32': prg.wave32,
      'resources': [prg.rsrc1, prg.rsrc2, prg.rsrc3],
      'descriptor_offset': prg.aql_prog_addr - prg.lib_gpu.va_addr,
      'entry_offset': prg.prog_addr - prg.lib_gpu.va_addr,
      'dispatch_pointer': bool(prg.enable_dispatch_ptr),
      'private_segment_sgpr': bool(prg.enable_private_segment_sgpr),
      'argument_allocation_size': prg.kernargs_segment_size + (ctypes.sizeof(hsa.hsa_kernel_dispatch_packet_t) if prg.enable_dispatch_ptr else 0),
    },
    'scratch': {'private_bytes': case['private'], 'bytes_per_xcc': dev.scratch.size // xccs, 'total_bytes': dev.scratch.size, 'tmpring': dev.tmpring_size},
    'aql_descriptor': list(bytes(dev.aql_desc)),
  }
  storage = ctypes.create_string_buffer(prg.kernargs_segment_size + 64)
  case['arguments'] = ctypes.addressof(storage)
  args = types.SimpleNamespace(buf=ArgBuffer(case['arguments']))
  global_size, local_size = [11, 7, 3], [8, 4, 2]
  queue = env['AMDComputeQueue'](dev)
  queue.exec(prg, args, global_size, local_size)
  expected['words'] = queue._q
  queue = env['AMDComputeQueue'](dev)
  queue.memory_barrier()
  expected['barrier'] = queue._q
  packet = hsa.hsa_kernel_dispatch_packet_t(
    workgroup_size_x=8,
    workgroup_size_y=4,
    workgroup_size_z=2,
    grid_size_x=88,
    grid_size_y=28,
    grid_size_z=6,
    private_segment_size=prg.private_segment_size,
    group_segment_size=prg.group_segment_size,
    kernarg_address=case['arguments'],
  )
  expected['dispatch'] = list(bytes(packet))
  if prg.enable_dispatch_ptr:
    assert bytes(storage)[prg.kernargs_segment_size :] == bytes(packet)
  queue = env['AMDComputeAQLQueue'](dev)
  queue.exec(prg, args, global_size, local_size)
  expected['aql'] = list(bytes(queue._q[0]))
  return expected


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--elf', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  args.evidence.mkdir(exist_ok=True, parents=True)
  _, sections, _ = elf_loader(args.elf.read_bytes())
  offset = next(section.header.sh_offset for section in sections if section.name == '.rodata')
  cases, expected = [], []
  for properties in [0, 1, 2, 3, 0x402]:
    data = bytearray(args.elf.read_bytes())
    struct.pack_into('<H', data, offset + 56, properties)
    path = args.evidence / f'properties-{properties}.elf'
    path.write_bytes(data)
    for gfx in [9, 11, 12]:
      for xccs in [1] if properties & 1 else [1, 2]:
        for private in [128, 777]:
          case = {'path': str(path), 'gfx': gfx, 'xccs': xccs, 'private': private}
          expected.append(original(case))
          cases.append(case)
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), text=True, capture_output=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  rows = [{'case': case, 'source': old, 'native': new, 'passed': old == new} for case, old, new in zip(cases, expected, actual, strict=True)]
  (args.evidence / 'comparison.json').write_text(json.dumps({'invocation': [args.binary], 'results': rows}, indent=2) + '\n')
  assert all(row['passed'] for row in rows), [index for index, row in enumerate(rows) if not row['passed']]
  print(f'PASS {len(rows)} source ELF resources/scratch/PM4 dispatch/AQL packet scenarios')


if __name__ == '__main__':
  main()
