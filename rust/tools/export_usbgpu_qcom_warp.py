from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--runtime', type=Path, required=True)
  parser.add_argument('--qemu', type=Path, required=True)
  parser.add_argument('--sysroot', type=Path, required=True)
  parser.add_argument('--python', type=Path, required=True)
  parser.add_argument('--compiler', type=Path, required=True)
  parser.add_argument('--width', type=int, required=True)
  parser.add_argument('--height', type=int, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  if shutil.disk_usage(args.output.parent).free < (25 << 30) + (32 << 20):
    raise OSError('QCOM warp export requires 25 GiB free plus 32 MiB bounded growth')
  compiler_hash = hashlib.sha256(args.compiler.read_bytes()).hexdigest()
  if compiler_hash != 'fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093':
    raise ValueError('QCOM compiler checksum mismatch')
  os.environ.update(DEV='NULL:QCOMCL:a630', BEAM='0', LRU='0')
  sys.path.insert(0, str(args.runtime))
  from tinygrad.device import Compiler, BufferStorage
  from tinygrad.runtime.support.compiler_qcom import QCOMCompiler

  processes = []

  def initialize(self, arch):
    self.arch, self.chip_id = arch, 0x6030001
    command = [str(args.qemu), '-cpu', 'max,pauth=off', '-L', str(args.sysroot), str(args.python),
               str(args.runtime / 'tinygrad/runtime/support/compileserver.py'),
               'tinygrad.runtime.support.compiler_qcom:QCOMCompiler', arch]
    self.compiler_process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0,
      env={**os.environ, 'PYTHONPATH': str(args.runtime), 'LLVM_QCOM_PATH': str(args.compiler)})
    processes.append(self.compiler_process)
    Compiler.__init__(self, None)

  QCOMCompiler.__init__ = initialize
  from tinygrad import Tensor, Device
  from examples.openpilot.compile_warp import NV12Frame, make_frame_prepare

  allocations, initialized, views, kernels, binaries, calls = [], {}, [], [], [], []

  def allocate(size, options):
    key = len(allocations)
    allocations.append({'bytes': size, 'weight_offset': None})
    return BufferStorage((key, 0, size))

  def offset(buffer, size, start):
    return BufferStorage((buffer[0], buffer[1] + start, size))

  def copyin(buffer, data):
    key, start, size = buffer
    target = initialized.setdefault(key, bytearray(allocations[key]['bytes']))
    target[start:start + len(data)] = data.cast('B')

  def view(buffer):
    key, start, size = buffer
    value = {'allocation': key, 'offset': start, 'bytes': size}
    if value not in views:
      views.append(value)
    return views.index(value)

  class Program:
    def __init__(self, device, obj):
      self.index = len(kernels)
      arguments = []
      for name, _slot, dtype, _shape in obj.signature:
        if name is not None:
          raise ValueError('QCOM warp symbolic scalar signature is unsupported')
        if dtype.__class__.__name__ == 'ImageDType':
          raise ValueError('QCOM warp image arguments require an explicit layout')
        arguments.append([{'kind': 'buffer'}])
      kernels.append({'name': obj.name, 'binary_sha256': hashlib.sha256(obj.lib).hexdigest(),
                      'binary_bytes': len(obj.lib), 'arguments': arguments})
      binaries.append(obj.lib)

    def __call__(self, *buffers, global_size=(1, 1, 1), local_size=(1, 1, 1), vals=(), **kwargs):
      calls.append({'op': 'kernel', 'kernel': self.index, 'views': [view(buffer) for buffer in buffers],
                    'scalars': list(vals), 'global': list(global_size), 'local': list(local_size)})
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
  weights = bytearray()
  for key, data in sorted(initialized.items()):
    allocations[key]['weight_offset'] = len(weights)
    weights.extend(data)
  def binding(name, tensor):
    buffer = tensor._buffer()
    return {'name': name, 'view': view(buffer.get_buf(device.device))}
  graph = {'version': 1, 'backend': 'qcom-cl', 'arch': 'a630', 'weights_sha256': hashlib.sha256(weights).hexdigest(),
           'allocations': allocations, 'views': views, 'kernels': kernels, 'calls': calls,
           'inputs': [binding('frames', frames), binding('transforms', transforms)], 'outputs': [binding('new_img', output)]}
  args.output.mkdir(parents=True, exist_ok=False)
  for index, data in enumerate(binaries):
    (args.output / f'kernel-{index}.bin').write_bytes(data)
  (args.output / 'weights.bin').write_bytes(weights)
  (args.output / 'graph.json').write_text(json.dumps(graph, indent=2) + '\n')
  provenance = {'camera': [width, height], 'frame_size': frame_size, 'compiler_sha256': compiler_hash,
                'source_sha256': hashlib.sha256((args.runtime / 'examples/openpilot/compile_warp.py').read_bytes()).hexdigest(),
                'scope': 'Original source warp compiled through pinned ARM64 QCOM compiler; no GPU execution or pixel acceptance.'}
  (args.output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
  for process in processes:
    process.terminate()
    process.wait(timeout=10)
  print(json.dumps({'output': str(args.output), 'kernels': len(kernels), 'calls': len(calls), 'weights_bytes': len(weights)}))


if __name__ == '__main__':
  main()
