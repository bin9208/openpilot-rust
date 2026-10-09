from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys


def main():
  parser = argparse.ArgumentParser(description='Compile the source Tensor.rand offroad probe using only the NULL AMD compiler.')
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--arch', choices=['gfx1200', 'gfx1201', 'gfx1100', 'gfx1101', 'gfx1102', 'gfx1150', 'gfx942', 'gfx950'], required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  os.environ.update(DEV=f'NULL:AMDLLVM:{args.arch}', BEAM='0', LRU='0')
  sys.path.insert(0, str(args.source))
  from tinygrad import Tensor, Device
  allocations, copies, kernels, calls = [], [], [], []

  def allocate(size, options):
    key = len(allocations)
    allocations.append(size)
    return key, 0

  def offset(buffer, offset, size):
    return buffer[0], buffer[1] + offset

  def copyin(buffer, data):
    copies.append({'buffer': buffer[0], 'offset': buffer[1], 'data': list(data.cast('B'))})

  class Program:
    def __init__(self, name, lib, *args, **kwargs):
      self.index = len(kernels)
      kernels.append({'name': name, 'elf': list(lib), 'sha256': hashlib.sha256(lib).hexdigest()})

    def __call__(self, *buffers, global_size=(1, 1, 1), local_size=(1, 1, 1), vals=(), **kwargs):
      calls.append({'kernel': self.index, 'buffers': [{'buffer': key, 'offset': off} for key, off in buffers],
                    'values': list(vals), 'global': list(global_size), 'local': list(local_size)})
      return 0.0

  device = Device.default
  device.allocator._alloc, device.allocator._offset, device.allocator._copyin = allocate, offset, copyin
  device.runtime = Program
  Tensor.manual_seed(42)
  output = Tensor.rand(1 << 20).realize()._buffer()
  key, offset = output._buf
  value = {'version': 1, 'arch': args.arch, 'seed': 42, 'buffers': allocations, 'copies': copies, 'kernels': kernels, 'calls': calls,
           'output': {'buffer': key, 'offset': offset, 'bytes': output.nbytes},
           'source_sha256': hashlib.sha256((args.source / 'tinygrad/mixin/rand.py').read_bytes()).hexdigest()}
  args.output.write_text(json.dumps(value, separators=(',', ':')) + '\n')
  print(json.dumps({'buffers': allocations, 'copies': copies, 'kernels': len(kernels), 'calls': len(calls), 'output': value['output']}))


if __name__ == '__main__':
  main()
