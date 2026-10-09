"""Opt-in correction for the source emulator's unaligned global 32-bit stores."""

from __future__ import annotations


def install(alignment_mode: int) -> None:
  """Preserve byte addresses through the source emulator's existing byte-store path."""
  from test.mockgpu.amd import emu
  from tinygrad.dtype import dtypes
  from tinygrad.uop.ops import UOp

  if alignment_mode != 3:
    raise GlobalStoreContractError('byte-addressed global stores require SH_MEM_CONFIG UNALIGNED mode')

  original = emu._mem_store

  # The hook retains the source memory-emitter signature and all other widths.
  def store(mem, addr, value, active, addr_bits=32, data_bits=32):
    if data_bits == 32:
      writes = []
      for index in range(4):
        byte = (value.cast(dtypes.uint32) >> UOp.const(dtypes.uint32, index * 8)) & UOp.const(dtypes.uint32, 0xFF)
        writes.extend(original(mem.after(*writes), addr + UOp.const(addr.dtype, index), byte, active, addr_bits, 8))
      return writes
    return original(mem, addr, value, active, addr_bits, data_bits)

  emu._mem_store = store


class GlobalStoreContractError(RuntimeError):
  pass
