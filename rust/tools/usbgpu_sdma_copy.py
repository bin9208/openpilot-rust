from __future__ import annotations
import ctypes
import struct


def copy(executor):
  packet = executor.base + executor.rptr[0] % executor.size
  _, count, _, source, destination = struct.unpack('<IIIQQ', ctypes.string_at(packet, 28))
  remaining, copied = (count & 0x3FFFFFFF) + 1, 0
  while remaining:
    size = min(remaining, 4096 - (source + copied) % 4096, 4096 - (destination + copied) % 4096)
    ctypes.memmove(executor.gpu.translate_addr(destination + copied), executor.gpu.translate_addr(source + copied), size)
    remaining, copied = remaining - size, copied + size
  executor.rptr[0] += 28


def install():
  from test.mockgpu.amd.amdgpu import SDMAExecutor

  SDMAExecutor._execute_copy = copy
