from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

from usbgpu_compute_fixture import Boundary as ComputeBoundary
from test.mockgpu.am import amgpu
from test.mockgpu.amd.amdgpu import PM4Executor
from tinygrad.helpers import DEV
from usbgpu_model_oracle import compare


class Boundary(ComputeBoundary):
  def __init__(self):
    DEV.value = 'AMD::gfx1200;CPU:LLVM'
    amgpu.VRAM_SIZE = 4 << 30
    super().__init__()
    self.pending = None
    self.kernels = 0
    original_dispatch = PM4Executor._exec_dispatch_direct

    def dispatch(queue, words):
      self.kernels += 1
      print(f'owned GPU kernel {self.kernels}', file=sys.stderr, flush=True)
      return original_dispatch(queue, words)

    PM4Executor._exec_dispatch_direct = dispatch

  def pci_read(self, address, size):
    if not 0x1_0000_0000 <= address <= 0x2_0000_0000 - size:
      raise ValueError(f'owned PCI read out of bounds: {address:#x}/{size}')
    return bytes(self.gpu.vram[address - 0x1_0000_0000 : address - 0x1_0000_0000 + size])

  def pci_write(self, address, data):
    if 0xCAFE0000 <= address < 0xCAFE2000:
      if (address - 0xCAFE0000) % 8 == 0:
        super().call({'op': 'doorbell_write', 'index': (address - 0xCAFE0000) // 8, 'value': int.from_bytes(data, 'little')})
      return
    if not 0x1_0000_0000 <= address <= 0x2_0000_0000 - len(data):
      raise ValueError(f'owned PCI write out of bounds: {address:#x}/{len(data)}')
    self.gpu.vram[address - 0x1_0000_0000 : address - 0x1_0000_0000 + len(data)] = data

  def call(self, request):
    if request['op'] not in {'usb_control', 'usb_bulk'}:
      return super().call(request)
    data = bytes(request['data'])
    if request['op'] == 'usb_control':
      if request['type'] != 0x40 or request['request'] != 0xF0 or len(data) != 12:
        raise ValueError('unsupported owned HCQ USB control')
      address, word = struct.unpack('<QI', data)
      mode = request['index']
      if mode == 0:
        self.pci_write(address, struct.pack('<I', word))
      elif mode in (1, 2):
        self.pending = address, word * 4, mode
      else:
        raise ValueError('unsupported F0 mode')
      response = {'code': len(data)}
    else:
      if self.pending is None:
        raise ValueError('bulk without preceding F0 command')
      address, size, mode = self.pending
      if size != len(data):
        raise ValueError('F0/bulk size mismatch')
      if request['endpoint'] == 0x81 and mode == 2:
        response = {'code': 0, 'actual': size, 'data': list(self.pci_read(address, size))}
      elif request['endpoint'] == 0x02 and mode == 1:
        self.pci_write(address, data)
        response = {'code': 0, 'actual': size}
      else:
        raise ValueError('F0/bulk direction mismatch')
      self.pending = None
    record = {**request, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    del record['data']
    recorded_response = response.copy()
    if 'data' in recorded_response:
      returned = bytes(recorded_response.pop('data'))
      recorded_response['size'] = len(returned)
      recorded_response['sha256'] = hashlib.sha256(returned).hexdigest()
    self.trace.append({**record, 'result': recorded_response})
    return response


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', required=True)
  parser.add_argument('--firmware', type=Path, required=True)
  parser.add_argument('--bundle', type=Path, required=True)
  parser.add_argument('--model', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--source-library', type=Path)
  parser.add_argument('--inspect-kernels', action='store_true')
  parser.add_argument('--inspect-fp', action='store_true')
  parser.add_argument('--capture-kernels')
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  with Path(args.binary).open('rb') as binary:
    binary_hash = hashlib.file_digest(binary, 'sha256').hexdigest()
  command = [args.binary, str(args.firmware), str(args.bundle), str(args.model), str(args.evidence / 'outputs.bin')]
  boundary, done, started = Boundary(), None, time.monotonic()
  fp = None
  if args.inspect_fp:
    from usbgpu_fp_inspect import Inspector as FpInspector

    fp = FpInspector(boundary, args.evidence)
    fp.install()
    os.environ['USBGPU_INSPECT_KERNELS'] = '1'
  if args.inspect_kernels:
    os.environ['USBGPU_INSPECT_KERNELS'] = '1'
  if args.capture_kernels:
    os.environ['USBGPU_INSPECT_KERNELS'] = '1'
  if args.source_library is not None:
    os.environ['USBGPU_SOURCE_ORACLE'] = '1'
  with (args.evidence / 'native-stderr.log').open('w') as stderr:
    with subprocess.Popen(
      command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, env={**os.environ, 'USBGPU_FIXTURE_VRAM_BYTES': str(4 << 30)}
    ) as process:
      try:
        for line in process.stdout:
          request = json.loads(line)
          if 'done' in request:
            done = request['done']
            break
          if request['op'] == 'inspect_bindings':
            if args.capture_kernels:
              from usbgpu_kernel_capture import Capture

              Capture(args, boundary, request['snapshot']).install()
            if fp is not None:
              fp.snapshot = request['snapshot']
            if args.inspect_kernels:
              from usbgpu_kernel_inspect import Inspector

              inspector = Inspector(args, boundary, request['snapshot'])
              inspector.install()
            response = True
          elif request['op'] == 'source_oracle':
            if args.source_library is None:
              raise ValueError('inspection fixture requires --source-library')
            response = compare(boundary, request['snapshot'], args.source_library, args.evidence)
          else:
            response = boundary.call(request)
          process.stdin.write(json.dumps({'value': response}) + '\n')
          process.stdin.flush()
        process.stdin.close()
        code = process.wait(timeout=10)
      finally:
        if process.poll() is None:
          process.kill()
          process.wait()
        (args.evidence / 'trace.json').write_text(json.dumps(boundary.trace, separators=(',', ':')))
  result = {
    'invocation': command,
    'exit_code': code,
    'result': done,
    'events': len(boundary.trace),
    'seconds': time.monotonic() - started,
    'kernels': boundary.kernels,
    'emulator_backend': 'CPU:LLVM',
    'binary_sha256': binary_hash,
    'fixture_sdma_copy': os.environ.get('USBGPU_FIXTURE_SDMA_COPY', 'page-aware'),
    'fixture_wmma_inline': os.environ.get('USBGPU_FIXTURE_WMMA_INLINE', 'owned-adapter'),
    'finite_output': done is not None and done['finite_f32'] * 4 == done['output_bytes'],
  }
  (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert code == 0 and done is not None and result['finite_output']


if __name__ == '__main__':
  main()
