from __future__ import annotations
import argparse
import ctypes
import json
import os
from pathlib import Path
import struct
import subprocess
import types

from usbgpu_runtime_fixture import Boundary as InitBoundary, Pci, prepare_source
from test.mockgpu.amd.pcode import Parser
from tinygrad.uop.ops import Ops, UOp


def current_variable_name(parser, base):
  if base.op == Ops.PARAM and base.arg.vmin_vmax is not None:
    return base.arg.name
  for name, value in parser.vars.items():
    if isinstance(value, UOp) and value is base:
      return name
  return None


Parser._find_var_name = current_variable_name


class Boundary(InitBoundary):
  def __init__(self):
    super().__init__()
    from usbgpu_sdma_copy import install

    if os.environ.get('USBGPU_FIXTURE_SDMA_COPY') != 'original':
      install()
    if os.environ.get('USBGPU_FIXTURE_WMMA_INLINE') != 'original':
      from usbgpu_wmma_inline import install as install_wmma

      install_wmma()

  def call(self, request):
    result = super().call(request)
    if request['op'] == 'doorbell_write':
      progress = True
      while progress:
        progress = False
        for queue in self.gpu.queues:
          if queue.executing:
            try:
              progress |= queue.execute() > 0
            except RuntimeError as error:
              if str(error) != 'Unknown SDMA op 2':
                raise

              def word(index, queue=queue):
                return queue.queue[(queue.rptr[0] // 4 + index) % (queue.size // 4)]

              count = word(3) + 1
              if count > 4096 or word(0) != 2:
                raise ValueError('invalid owned SDMA WRITE packet') from error
              address = word(1) | (word(2) << 32)
              data = struct.pack(f'<{count}I', *[word(4 + i) for i in range(count)])
              ctypes.memmove(self.gpu.translate_addr(address), data, len(data))
              queue.rptr[0] += 16 + len(data)
              progress = True
    return result


def original(boundary, firmware, kernel):
  from tinygrad.runtime import ops_amd
  from tinygrad.runtime.support import hcq

  clock = prepare_source(boundary, firmware, None)
  ops_amd.time = hcq.time = clock
  ops_amd.USB3 = types.SimpleNamespace(list_devices=lambda vendor, product: [(object(), 'fixture')] if vendor == 0xADD1 else [])
  ops_amd.USBPCIDevice = lambda *args: Pci(boundary, True)
  ops_amd.System.memory_barrier = lambda: boundary.call({'op': 'barrier'})
  ops_amd.AMDDevice.ifaces = [ops_amd.USBIface]
  os.environ['AMD_AQL'], os.environ['AMD_DISABLE_SDMA'] = '0', '0'
  device = ops_amd.AMDDevice('AMD')
  program = ops_amd.AMDProgram(device, 'affine', kernel.read_bytes())
  source, destination = device.allocator.alloc(128), device.allocator.alloc(128)
  device.allocator._copyin(source, memoryview(struct.pack('<32f', *[float(i - 16) for i in range(32)])))
  program(destination, source, global_size=(1, 1, 1), local_size=(32, 1, 1))
  output = bytearray(128)
  device.allocator._copyout(memoryview(output), destination)
  result = {'output': list(output), 'timeline': device.timeline_value}
  del program
  device.iface.dev_impl.fini()
  return result


def native(boundary, firmware, kernel, binary):
  command = [binary, str(firmware), str(kernel)]
  process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
  done = None
  for line in process.stdout:
    request = json.loads(line)
    if 'done' in request:
      done = request['done']
      break
    response = {'value': boundary.call(request)}
    process.stdin.write(json.dumps(response) + '\n')
    process.stdin.flush()
  process.stdin.close()
  stderr = process.stderr.read()
  code = process.wait(timeout=10)
  if code != 0:
    raise RuntimeError(f'native compute failed: {code}: {stderr}')
  return {**done, 'invocation': command, 'exit_code': code}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--kind', choices=['source', 'native'], required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--kernel', type=Path, required=True)
  parser.add_argument('--binary')
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  boundary = Boundary()
  result = original(boundary, args.firmware, args.kernel) if args.kind == 'source' else native(boundary, args.firmware, args.kernel, args.binary)
  expected = list(struct.pack('<32f', *[float(2 * (i - 16) + 1) for i in range(32)]))
  args.output.write_text(json.dumps({'result': result, 'expected': expected, 'trace': boundary.trace}, indent=2) + '\n')
  assert result['output'] == expected
  print(args.kind, 'PASS', len(boundary.trace), 'events; 32 exact f32 outputs')


if __name__ == '__main__':
  main()
