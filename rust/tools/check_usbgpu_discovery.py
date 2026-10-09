"""Validate native AMD discovery/register binding against original C layouts."""

from __future__ import annotations
import argparse
import ast
import ctypes
import json
from pathlib import Path
import struct
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / 'tinygrad_repo'))
from tinygrad.runtime.autogen.am import am
from tinygrad.runtime.support.am.amdev import AMDev, AMRegister


class Vram:
  nbytes = 512 << 20

  def __init__(self, table):
    self.table = table

  def view(self, offset, size):
    assert offset == self.nbytes - (64 << 10) and size == 10 << 10
    return self.table


def fixtures():
  path = ROOT / 'tinygrad_repo/test/mockgpu/am/amgpu.py'
  tree = ast.parse(path.read_text())
  tree.body = [
    node
    for node in tree.body
    if isinstance(node, ast.FunctionDef)
    and node.name == '_pad'
    or isinstance(node, ast.Assign)
    and any(isinstance(target, ast.Name) and target.id in {'IP_VERSIONS', 'IP_BASES', 'GC_INFO'} for target in node.targets)
  ]
  env = {'am': am}
  exec(compile(tree, str(path), 'exec'), env)
  for wide in [False, True]:
    for instances in [1, 2]:
      for gc_version in [(2, 0), (2, 1)]:
        ip_data = bytearray()
        for hwip, version in env['IP_VERSIONS'].items():
          for instance in range(instances):
            ip = am.struct_ip_v4(
              hw_id=am.hw_id_map[hwip],
              instance_number=instance,
              num_base_address=len(env['IP_BASES'][hwip]),
              major=version[0],
              minor=version[1],
              revision=version[2],
            )
            ip_data += bytes(ip) + b'\0'
            for base in env['IP_BASES'][hwip]:
              ip_data += struct.pack('<Q' if wide else '<I', base + instance * 0x1000)
        die = am.struct_die_header(num_ips=len(env['IP_VERSIONS']) * instances)
        header = am.struct_ip_discovery_header(signature=am.DISCOVERY_TABLE_SIGNATURE, version=4, num_dies=1, base_addr_64_bit=wide)
        header.die_info[0].die_offset = ctypes.sizeof(am.struct_binary_header) + ctypes.sizeof(header)
        gc = getattr(am, f'struct_gc_info_v{gc_version[0]}_{gc_version[1]}')()
        gc.header.table_id, gc.header.version_major, gc.header.version_minor, gc.header.size = am.GC, *gc_version, ctypes.sizeof(gc)
        for name, value in env['GC_INFO'].items():
          if hasattr(gc, name):
            setattr(gc, name, value)
        binary = am.struct_binary_header(binary_signature=am.BINARY_SIGNATURE)
        binary.table_list[am.IP_DISCOVERY].offset = ctypes.sizeof(binary)
        binary.table_list[am.GC].offset = ctypes.sizeof(binary) + ctypes.sizeof(header) + ctypes.sizeof(die) + len(ip_data)
        data = bytes(binary) + bytes(header) + bytes(die) + ip_data + bytes(gc)
        yield data.ljust(10 << 10, b'\0')


def original(data):
  device = types.SimpleNamespace(vram=Vram(data), rreg=lambda reg: 512)
  AMDev._run_discovery(device)
  AMDev._build_regs(device)
  return {
    'discovery': {
      'versions': {str(key): list(value) for key, value in device.ip_ver.items()},
      'bases': {str(key): {str(index): list(values) for index, values in group.items()} for key, group in device.regs_offset.items()},
      'gc_version': [device.gc_info.header.version_major, device.gc_info.header.version_minor],
      'gc_info': {name: getattr(device.gc_info, name) for name in type(device.gc_info).__annotations__ if name.startswith('gc_')},
    },
    'registers': {
      name: {'addresses': {str(key): value for key, value in register.addr.items()}, 'fields': {key: list(value) for key, value in register.fields.items()}}
      for name, register in vars(device).items()
      if isinstance(register, AMRegister)
    },
  }


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', required=True, type=Path)
  args = parser.parse_args()
  cases = list(fixtures())
  process = subprocess.run([args.binary], input=''.join(json.dumps(list(case)) + '\n' for case in cases), capture_output=True, text=True, timeout=30)
  assert process.returncode == 0, process.stderr
  results = []
  for index, (data, line) in enumerate(zip(cases, process.stdout.splitlines(), strict=True)):
    native, source = json.loads(line), original(data)
    results.append({'case': index, 'source': source, 'native': native, 'passed': source == native})
  args.evidence.write_text(json.dumps({'invocation': [args.binary], 'results': results}, indent=2) + '\n')
  assert all(result['passed'] for result in results), [result['case'] for result in results if not result['passed']]
  print(f'PASS {len(results)} original AMD discovery/register binding cases')


if __name__ == '__main__':
  main()
