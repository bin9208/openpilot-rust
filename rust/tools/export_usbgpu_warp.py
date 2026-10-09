"""Compile the pinned source NV12 warp to AMD kernels without opening a device."""

from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--runtime', type=Path, required=True)
  parser.add_argument('--width', type=int, required=True)
  parser.add_argument('--height', type=int, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  os.environ['DEV'] = 'NULL:AMDLLVM:gfx1200'
  os.environ['BEAM'] = '0'
  os.environ['LRU'] = '0'
  sys.path.insert(0, str(args.runtime))
  from tinygrad import Tensor, Device
  from tinygrad.device import BufferStorage
  from examples.openpilot.compile_warp import NV12Frame, make_frame_prepare

  allocations, copies, calls, kernels = [], [], [], []

  def allocate(size, options):
    key = len(allocations)
    allocations.append(size)
    return BufferStorage((key, 0))

  def offset(buffer, size, offset):
    return BufferStorage((buffer[0], buffer[1] + offset))

  def copyin(buffer, data):
    copies.append({'buffer': buffer[0], 'offset': buffer[1], 'data': list(data.cast('B'))})

  class Program:
    def __init__(self, device, obj):
      self.index = len(kernels)
      kernels.append({'name': obj.name, 'elf': list(obj.lib), 'sha256': hashlib.sha256(obj.lib).hexdigest()})

    def __call__(self, *buffers, global_size=(1, 1, 1), local_size=(1, 1, 1), vals=(), **kwargs):
      calls.append(
        {
          'kernel': self.index,
          'buffers': [{'buffer': key, 'offset': off} for key, off in buffers],
          'values': list(vals),
          'global': list(global_size),
          'local': list(local_size),
        }
      )
      return 0.0

  device = Device.default
  device.allocator._alloc, device.allocator._offset, device.allocator._copyin = allocate, offset, copyin
  device.runtime_t = Program
  width, height = args.width, args.height
  if (width, height) not in ((1928, 1208), (1344, 760)):
    raise ValueError('unsupported camera dimensions')
  stride, y_height, uv_height = ((width + 127) // 128) * 128, ((height + 31) // 32) * 32, ((height // 2 + 15) // 16) * 16
  frame_size = stride * (y_height + uv_height)
  frame = NV12Frame(width, height, stride, y_height, uv_height, frame_size)
  frames = Tensor.empty(2, frame_size, dtype='uint8').realize()
  transforms = Tensor.empty(2, 3, 3, dtype='float32').realize()
  prepare = make_frame_prepare(frame, 512, 256)
  output = Tensor.stack(*(prepare(frames[i], transforms[i]) for i in range(2))).realize()

  def view(tensor):
    buffer = tensor._buffer()
    key, offset = buffer.get_buf(device.device)
    return {'buffer': key, 'offset': offset, 'bytes': buffer.nbytes}

  result = {
    'version': 1,
    'arch': 'gfx1200',
    'camera': [width, height],
    'frame_size': frame_size,
    'buffers': allocations,
    'copies': copies,
    'kernels': kernels,
    'calls': calls,
    'inputs': {'frames': view(frames), 'transforms': view(transforms)},
    'output': view(output),
    'source_sha256': hashlib.sha256((args.runtime / 'examples/openpilot/compile_warp.py').read_bytes()).hexdigest(),
  }
  args.output.write_text(json.dumps(result, separators=(',', ':')) + '\n')
  print(json.dumps({'buffers': len(allocations), 'kernels': len(kernels), 'calls': len(calls), 'output': result['output']}))


if __name__ == '__main__':
  main()
