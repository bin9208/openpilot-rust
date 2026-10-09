"""Restore captured virtual addresses without replacing any existing mapping."""

from __future__ import annotations

import ctypes
import mmap
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass


class MappingContractError(OSError):
  pass


@dataclass(frozen=True, slots=True)
class Image:
  device: int
  data: bytes


@contextmanager
def restore(images: list[Image]) -> Iterator[None]:
  """Own fixed, non-replacing anonymous mappings until the replay exits."""
  library = ctypes.CDLL(None, use_errno=True)
  library.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
  library.mmap.restype = ctypes.c_void_p
  library.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
  library.munmap.restype = ctypes.c_int
  mappings = []
  if sum(len(image.data) for image in images) > 104 << 20:
    raise MappingContractError('captured input images exceed 104 MiB replay limit')
  try:
    for image in images:
      if not image.data or image.device < mmap.PAGESIZE:
        raise MappingContractError('captured image has an invalid address or empty range')
      address = image.device // mmap.PAGESIZE * mmap.PAGESIZE
      offset = image.device - address
      size = (offset + len(image.data) + mmap.PAGESIZE - 1) // mmap.PAGESIZE * mmap.PAGESIZE
      # Linux SDK MAP_FIXED_NOREPLACE: failure must never clobber another allocation.
      pointer = library.mmap(address, size, mmap.PROT_READ | mmap.PROT_WRITE, mmap.MAP_PRIVATE | mmap.MAP_ANONYMOUS | 0x100000, -1, 0)
      if pointer != address:
        if pointer not in (None, ctypes.c_void_p(-1).value):
          library.munmap(pointer, size)
        raise MappingContractError(ctypes.get_errno(), 'captured address could not be reserved without replacement')
      mappings.append((address, size))
      ctypes.memmove(image.device, image.data, len(image.data))
    yield
  finally:
    for address, size in reversed(mappings):
      if library.munmap(address, size) != 0:
        raise MappingContractError(ctypes.get_errno(), 'owned replay mapping could not be released')
