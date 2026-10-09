"""Compare stock ASM command generation through an owned SCSI boundary."""

from __future__ import annotations
import argparse
import ast
import dataclasses
import itertools
import json
from pathlib import Path
import struct
import subprocess
import sys
import types

ROOT = Path(__file__).resolve().parents[2]


class ScsiFixture:
  def __init__(self):
    self.registers = {}
    self.trace = []

  def send_batch(self, commands, idata=None, odata=None):
    results = []
    for cdb, read, write in zip(commands, idata or [0] * len(commands), odata or [None] * len(commands), strict=True):
      self.trace.append({'cdb': list(cdb), 'read': read, 'write': list(write) if write is not None else None})
      address = int.from_bytes(cdb[2:5], 'big') & 0x1FFFF if len(cdb) == 6 else 0
      if cdb[0] == 0xE5:
        self.registers[address] = cdb[1]
        if address == 0xB296 and cdb[1] == 4:
          fmt, enable = self.registers[0xB210], self.registers[0xB217]
          self.registers.update({0xB296: 2, 0xB284: int(fmt & 0x40 == 0), 0xB22A: 0, 0xB22B: 4 if fmt & 0xBE == 4 else enable.bit_count()})
          if fmt & 0x40 == 0:
            self.registers.update({0xB220 + index: value for index, value in enumerate([0x12, 0x34, 0x56, 0x78])})
      results.append(bytes(self.registers.get(address + offset, (address + offset) & 255) for offset in range(read)) if read else None)
    return results


def source(operations):
  path = ROOT / 'tinygrad_repo/tinygrad/runtime/support/usb.py'
  tree = ast.parse(path.read_text())
  tree.body = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)] + [
    node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in {'ASM24Controller', 'ReadOp', 'WriteOp', 'ScsiWriteOp'}
  ]
  ast.fix_missing_locations(tree)
  module = types.ModuleType('stock_asm_oracle')
  sys.modules[module.__name__] = module
  module.__dict__.update(dataclasses=dataclasses, struct=struct, itertools=itertools, DEBUG=0, OSX=False, round_up=lambda n, d: (n + d - 1) // d * d)
  exec(compile(tree, str(path), 'exec'), module.__dict__)
  usb = ScsiFixture()
  controller = module.ASM24Controller(usb)
  values = []
  for operation in operations:
    address, length = operation.get('address', 0), operation.get('length', 0)
    match operation['kind']:
      case 'read':
        value = list(controller.read(address, length, operation.get('stride', 255)))
      case 'write':
        controller.write(address, bytes(operation['data']), operation['ignore_cache'])
        value = None
      case 'cache':
        controller._pci_cacheable.append((address, length))
        value = None
      case 'request':
        value = controller.pcie_request(operation['format'], address, operation.get('value'), operation['size'])
      case 'memory_write':
        value = controller.pcie_mem_write(address, operation['data'], operation['size'])
      case 'scsi_write':
        value = controller.scsi_write(bytes(operation['data']), operation.get('lba', 0))
      case _:
        raise ValueError(operation)
    values.append(value)
  return {'values': values, 'trace': usb.trace}


def scenarios():
  yield []
  for length in [0, 1, 255, 256, 31 * 255, 32 * 255, 33 * 255, 62 * 255]:
    yield [{'kind': 'read', 'address': 0x1200, 'length': length}]
  yield [{'kind': 'write', 'address': 0x200, 'data': [1, 2, 3], 'ignore_cache': False}] * 2 + [
    {'kind': 'read', 'address': 0x200, 'length': 2},
    {'kind': 'write', 'address': 0x200, 'data': [1, 2, 3], 'ignore_cache': False},
  ]
  for offset in range(4):
    for size in range(1, 5 - offset):
      yield [{'kind': 'request', 'format': 0x20, 'address': 0x123400 + offset, 'size': size}]
      yield [{'kind': 'request', 'format': 0x60, 'address': 0x123400 + offset, 'size': size, 'value': 0x12}]
  for fmt in [4, 5, 0x44, 0x45]:
    yield [{'kind': 'request', 'format': fmt, 'address': 0x1000100, 'size': 4, **({'value': 12} if fmt & 0x40 else {})}]
  yield [{'kind': 'cache', 'address': 0x1000, 'length': 4}] + [{'kind': 'request', 'format': 0x60, 'address': 0x1000, 'size': 4, 'value': 12}] * 2
  for count in [0, 1, 16, 17, 32, 33]:
    yield [{'kind': 'memory_write', 'address': 0x100000000, 'data': list(range(count)), 'size': 4}]
  for length in [0, 1, 511, 512, 513, 16384, 16385, 65537]:
    yield [{'kind': 'scsi_write', 'data': [7] * length, 'lba': 3}]


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  args = parser.parse_args()
  cases = list(scenarios())
  process = subprocess.run([args.binary], input=''.join(json.dumps(case) + '\n' for case in cases), capture_output=True, text=True, timeout=30)
  assert process.returncode == 0, process.stderr
  actual = [json.loads(line) for line in process.stdout.splitlines()]
  results = []
  for case, native in zip(cases, actual, strict=True):
    expected = source(case)
    results.append({'operations': case, 'source': expected, 'native': native, 'passed': native == expected})
  args.evidence.write_text(
    json.dumps({'invocation': [args.binary], 'boundary': 'SCSI send_batch, not inherited USB3 result-window implementation', 'results': results}, indent=2)
    + '\n'
  )
  failures = [index for index, result in enumerate(results) if not result['passed']]
  assert not failures, failures
  print(f'PASS {len(results)} stock ASM controller source comparisons at SCSI boundary')


if __name__ == '__main__':
  main()
