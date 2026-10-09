"""Replay one captured original shader through the original PM4 executor."""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import math
from pathlib import Path
import struct
import time

# Reuse the recorded model fixture's PARAM parser compatibility boundary.
from usbgpu_compute_fixture import Parser, current_variable_name
from usbgpu_kernel_memory import Image, restore
from usbgpu_wmma_inline import install
from test.mockgpu.amd.amdgpu import (
  AMDGPU,
  AMDGPURegisters,
  PM4Executor,
  regCOMPUTE_NUM_THREAD_X,
  regCOMPUTE_PGM_LO,
  regCOMPUTE_PGM_RSRC2,
  regCOMPUTE_TMPRING_SIZE,
  regCOMPUTE_USER_DATA_0,
)
from tinygrad.helpers import DEV


class ReplayContractError(RuntimeError):
  pass


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--capture', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--global-store', choices=['original', 'byte-addressed'], default='original')
  args = parser.parse_args()
  receipt = json.loads((args.capture / 'producer-capture.json').read_text())
  images = []
  for backing in receipt['before']:
    name = backing['file']
    if Path(name).name != name:
      raise ReplayContractError('capture filename leaves its owned directory')
    data = (args.capture / name).read_bytes()
    if len(data) != backing['bytes'] or hashlib.sha256(data).hexdigest() != backing['sha256']:
      raise ReplayContractError('captured backing storage checksum or size mismatch')
    images.append(Image(backing['device'], data))
  argument_block = bytes.fromhex(receipt['arguments_hex'])
  address = receipt['arguments_pointer']
  covered = next((image for image in images if image.device <= address and address + len(argument_block) <= image.device + len(image.data)), None)
  if covered is None:
    images.append(Image(address, argument_block))
  elif covered.data[address - covered.device : address - covered.device + len(argument_block)] != argument_block:
    raise ReplayContractError('argument block conflicts with aliased captured backing storage')
  DEV.value = 'AMD::gfx1200;CPU:LLVM'
  Parser._find_var_name = current_variable_name
  install()
  if args.global_store == 'byte-addressed':
    from usbgpu_global_store import install as install_store

    install_store(3)
  gpu = AMDGPU.__new__(AMDGPU)
  gpu.arch, gpu.regs = receipt['arch'], AMDGPURegisters()
  gpu.mapped_ranges = {(image.device, len(image.data)) for image in images}
  gpu.regs[regCOMPUTE_PGM_LO], gpu.regs[regCOMPUTE_PGM_LO + 1] = (receipt['program'] >> 8) & 0xFFFFFFFF, receipt['program'] >> 40
  gpu.regs[regCOMPUTE_PGM_RSRC2] = receipt['rsrc2']
  gpu.regs[regCOMPUTE_TMPRING_SIZE] = (receipt['scratch_size'] // (16 if gpu.arch == 'cdna' else 4)) << 12
  for index, value in enumerate(receipt['user_data']):
    gpu.regs[regCOMPUTE_USER_DATA_0 + index] = value
  for index, value in enumerate(receipt['local']):
    gpu.regs[regCOMPUTE_NUM_THREAD_X + index] = value
  words = (ctypes.c_uint32 * len(receipt['dispatch_words']))(*receipt['dispatch_words'])
  rptr, wptr = (ctypes.c_uint64 * 1)(0), (ctypes.c_uint64 * 1)(len(words))
  queue = PM4Executor(gpu, ctypes.addressof(words), ctypes.sizeof(words), ctypes.addressof(rptr), ctypes.addressof(wptr))
  started = time.monotonic()
  with restore(images):
    PM4Executor._exec_dispatch_direct(queue, len(words) - 1)
    output = ctypes.string_at(receipt['output']['pointer'], receipt['output']['bytes'])
  expected = (args.capture / receipt['output']['file']).read_bytes()
  if len(expected) != receipt['output']['bytes'] or hashlib.sha256(expected).hexdigest() != receipt['output']['sha256']:
    raise ReplayContractError('captured output checksum or size mismatch')
  args.output.mkdir(parents=True, exist_ok=True)
  (args.output / 'output.bin').write_bytes(output)
  values = [value[0] for value in struct.iter_unpack('<' + receipt['output']['dtype'], output)]
  result = {
    'global_store': args.global_store,
    'kernel': receipt['kernel'],
    'bytes': len(output),
    'exact': output == expected,
    'sha256': hashlib.sha256(output).hexdigest(),
    'nan': sum(math.isnan(value) for value in values),
    'finite': sum(math.isfinite(value) for value in values),
    'seconds': time.monotonic() - started,
    'scope': 'Original PM4 executor and shader on captured aliased backing storage/registers/argument block; '
    + 'shared emulator, not full model or independent loader parity.',
  }
  (args.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  if args.global_store == 'original' and output != expected:
    raise ReplayContractError('original single-shader replay differs from captured output')


if __name__ == '__main__':
  main()
