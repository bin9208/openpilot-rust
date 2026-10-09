from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tinygrad_repo'))
from test.mockgpu.amd import emu
from tinygrad.device import Buffer, BufferSpec
from tinygrad.dtype import dtypes
from tinygrad.helpers import DEV
from tinygrad.renderer.amd import decode_inst
from tinygrad.renderer.amd.dsl import src, v


def execute(instruction, value, captured=None):
  state = emu.WaveState(32)
  prefix = 256 * 32 * 4
  backing = bytearray(struct.pack('<I', 0x7FC00000) * (prefix // 4) + bytes(prefix))
  host = (ctypes.c_ubyte * len(backing)).from_buffer(backing)
  state.vgpr_buf = Buffer('CPU', prefix // 4, dtypes.uint32,
    options=BufferSpec(external_ptr=ctypes.addressof(host) + prefix)).ensure_allocated()
  state.accvgpr_buf = state.vgpr_buf
  state._vgpr_mv = memoryview(backing)[prefix:].cast('I')
  for register in [*range(31, 35), *range(4)]:
    for lane in range(32):
      state._write_vgpr(register, lane, 0x3F803F80 if 'BF16' in instruction.op.name else 0x3C003C00)
  if captured is not None:
    state._vgpr_mv[:] = memoryview(captured).cast('I')
  if instruction.src2.offset >= 256:
    for register in range(40, 48):
      for lane in range(32):
        state._write_vgpr(register, lane, struct.unpack('<I', struct.pack('<f', value))[0])
  code = ctypes.create_string_buffer(instruction.to_bytes() + bytes(16))
  state.pc = ctypes.addressof(code)
  program, runtime = emu._get_runner(code.raw, 'rdna4')
  pointers = {0: state.sgpr_buf._buf.va_addr, 1: state.vgpr_buf._buf.va_addr, 2: 0}
  with emu._MXCSRContext():
    runtime.fxn(*[ctypes.c_uint64(pointers[index]) for index in program.arg.globals])
  return bytes(state._vgpr_mv[23 * 32:31 * 32].cast('B'))


def scalar_oracle(registers, output):
  words = struct.unpack('<8192I', registers)
  def matrix(base, row, k):
    element = (k & 3) | ((k >> 1) & 4)
    lane = row + ((k >> 2) & 1) * 16
    word = words[(base + element // 2) * 32 + lane]
    return struct.unpack('<e', struct.pack('<H', word >> (16 * (element % 2)) & 0xFFFF))[0]
  values = [sum(matrix(31, row, k) * matrix(0, column, k) for k in range(16))
            for row in range(16) for column in range(16)]
  inputs = [matrix(base, row, k) for base in (31, 0) for row in range(16) for k in range(16)]
  expected = [0.0] * 256
  for row in range(16):
    for column in range(16):
      accumulator = 0.0
      for k in range(16):
        accumulator = struct.unpack('<f', struct.pack('<f', accumulator + matrix(31, row, k) * matrix(0, column, k)))[0]
      expected[(row & 7) * 32 + column + (row >> 3) * 16] = accumulator
  expected_bytes = struct.pack('<256f', *expected)
  return {'input_values': len(inputs), 'finite_inputs': sum(map(math.isfinite, inputs)),
          'output_values': len(values), 'finite_outputs': sum(map(math.isfinite, values)),
          'minimum': min(values), 'maximum': max(values), 'exact_sequential_f32': output == expected_bytes,
          'expected_sha256': hashlib.sha256(expected_bytes).hexdigest(), 'observed_sha256': hashlib.sha256(output).hexdigest()}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--captured', type=Path, required=True)
  parser.add_argument('--evidence', type=Path, required=True)
  parser.add_argument('--adapter', action='store_true')
  args = parser.parse_args()
  DEV.value = 'CPU:LLVM'
  instruction = decode_inst((args.captured / 'instruction.bin').read_bytes(), 'rdna4')
  assert instruction.src2.offset == 128
  if args.adapter:
    from usbgpu_wmma_inline import install

    install()
  args.evidence.mkdir(parents=True, exist_ok=True)
  rows = []
  operands = [('inline-zero', 0, 0.0), ('vgpr-zero', v[40:47], 0.0), ('vgpr-two', v[40:47], 2.0)]
  operands += [(f'inline-{encoding}', src[encoding], struct.unpack('<f', struct.pack('<I', bits))[0])
               for encoding, bits in emu.F32_INLINE.items()]
  for kind, operation in [('f16', emu.ir4.VOP3POp.V_WMMA_F32_16X16X16_F16), ('bf16', emu.ir4.VOP3POp.V_WMMA_F32_16X16X16_BF16)]:
    for name, operand, value in operands:
      case = decode_inst(instruction.to_bytes(), 'rdna4')
      case.op, case.src2 = operation, operand
      output = execute(case, value)
      (args.evidence / f'{kind}-{name}.bin').write_bytes(output)
      values = struct.unpack('<256f', output)
      expected = struct.unpack('<f', struct.pack('<f', 16.0 + value))[0]
      rows.append({'name': f'{kind}-{name}', 'instruction': repr(case), 'src2_encoding': case.src2.offset,
        'finite': sum(map(math.isfinite, values)), 'exact_scalar_oracle': all(x == expected for x in values),
        'sha256': hashlib.sha256(output).hexdigest()})
  registers = (args.captured / 'vgpr-after.bin').read_bytes()
  captured_output = execute(instruction, 0.0, registers)
  (args.evidence / 'captured-operands.bin').write_bytes(captured_output)
  result = {'implementation': 'owned-inline-adapter' if args.adapter else 'original-emulator',
    'instruction_hex': instruction.to_bytes().hex(), 'rows': rows, 'captured_scalar_oracle': scalar_oracle(registers, captured_output),
    'scope': 'Actual decoded WMMA execution with finite A/B, deliberately poisoned pre-VGPR allocation and independent scalar dot oracle.'}
  result['passed'] = all(row['exact_scalar_oracle'] for row in rows) and result['captured_scalar_oracle']['exact_sequential_f32'] if args.adapter else (
    all(not row['exact_scalar_oracle'] and row['finite'] == 0 for row in rows if row['src2_encoding'] < 256) and
    all(row['exact_scalar_oracle'] for row in rows if row['src2_encoding'] >= 256))
  (args.evidence / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps(result))
  assert result['passed']


if __name__ == '__main__':
  main()
