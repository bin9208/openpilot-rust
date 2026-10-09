"""Compare actual AMD global-store instructions with their expected byte writes."""

from __future__ import annotations

import argparse
import ctypes
from dataclasses import dataclass
from enum import IntEnum
import json
from pathlib import Path
import struct
from typing import assert_never

from usbgpu_compute_fixture import Parser, current_variable_name
from test.mockgpu.amd.amdgpu import remu
from tinygrad.helpers import DEV
from tinygrad.runtime.autogen.amd.rdna4.ins import (
  global_store_b16,
  global_store_b32,
  s,
  s_endpgm,
  s_load_b64,
  s_wait_loadcnt,
  v,
  v_mov_b32_e32,
)


class Width(IntEnum):
  HALF = 16
  WORD = 32


@dataclass(frozen=True, slots=True)
class Case:
  width: Width
  offset: int


def run(case: Case):
  initial = bytes(range(16))
  storage = (ctypes.c_ubyte * len(initial)).from_buffer_copy(initial)
  args = (ctypes.c_uint64 * 1)(ctypes.addressof(storage) + case.offset)
  value = 0xA1B2C3D4
  match case.width:
    case Width.HALF:
      store = global_store_b16(vaddr=v[0:1], vsrc=v[2])
    case Width.WORD:
      store = global_store_b32(vaddr=v[0:1], vsrc=v[2])
    case unreachable:
      assert_never(unreachable)
  instructions = [
    s_load_b64(s[2:3], s[0:1]),
    s_wait_loadcnt(0),
    v_mov_b32_e32(v[0], s[2]),
    v_mov_b32_e32(v[1], s[3]),
    v_mov_b32_e32(v[2], value),
    store,
    s_endpgm(),
  ]
  code = b''.join(instruction.to_bytes() for instruction in instructions)
  program = (ctypes.c_ubyte * len(code)).from_buffer_copy(code)
  remu.valid_mem_ranges = {(ctypes.addressof(storage), len(initial)), (ctypes.addressof(program), len(code)), (ctypes.addressof(args), ctypes.sizeof(args))}
  remu.arch, remu.rsrc2, remu.scratch_size = 'rdna4', 4, 0
  remu.user_data = [ctypes.addressof(args) & 0xFFFFFFFF, ctypes.addressof(args) >> 32]
  status = remu.run_asm(ctypes.addressof(program), len(code), 1, 1, 1, 1, 1, 1, ctypes.addressof(args))
  expected = bytearray(initial)
  encoded = struct.pack('<I', value)[: case.width // 8]
  expected[case.offset : case.offset + len(encoded)] = encoded
  actual = bytes(storage)
  return {
    'width': int(case.width),
    'offset': case.offset,
    'exit': status,
    'exact': actual == expected,
    'actual_hex': actual.hex(),
    'expected_hex': expected.hex(),
    'program_hex': code.hex(),
  }


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--global-store', choices=['original', 'byte-addressed'], default='original')
  args = parser.parse_args()
  DEV.value = 'AMD::gfx1200;CPU:LLVM'
  Parser._find_var_name = current_variable_name
  if args.global_store == 'byte-addressed':
    from usbgpu_global_store import install

    install(3)
  rows = [run(Case(width, offset)) for width in Width for offset in range(4)]
  result = {
    'global_store': args.global_store,
    'passed': all(row['exit'] == 0 and row['exact'] for row in rows),
    'rows': rows,
    'scope': 'Actual original RDNA4 store instructions against expected little-endian bytes, including untouched neighbors; owned CPU emulator only.',
  }
  args.output.write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  if not result['passed']:
    raise AssertionError('source emulator store bytes differ from instruction semantics')


if __name__ == '__main__':
  main()
