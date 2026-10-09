from __future__ import annotations
import ctypes
import json
import struct
from types import SimpleNamespace

from usbgpu_model_oracle import virtual_bytes


class FirstNaN(RuntimeError):
  pass


class Inspector:
  def __init__(self, boundary, evidence):
    self.boundary, self.evidence, self.waves = boundary, evidence, {}
    self.snapshot = None
    self.executed = 0

  def install(self):
    from test.mockgpu.amd import emu
    from tinygrad.renderer.amd.dsl import EXEC_LO

    initialize, decode = emu._init_wave, emu._decode_at

    def init_wave(*args, **kwargs):
      state = initialize(*args, **kwargs)
      self.waves[state.vgpr_buf._buf.va_addr] = {'wave_size': state.wave_size, 'lanes': state.n_lanes,
        'program': args[0], 'wave_start': args[1], 'group': list(args[10:13]), 'args_pointer': args[6]}
      return state

    def decode_at(pc, arch):
      (program, runtime), instruction = decode(pc, arch)
      destination = instruction.canonical_operands.get('d')
      if destination is None or not hasattr(instruction, 'vdst') or 1 not in program.arg.globals:
        return (program, runtime), instruction
      fmt = destination[0].name
      if not any(kind in fmt for kind in ('F32', 'F16', 'BF16')):
        return (program, runtime), instruction
      register = instruction.vdst.offset - 256
      if not 0 <= register < 256:
        return (program, runtime), instruction
      count = max(1, destination[1] // 32)
      packed = 'PK2' in fmt or 'WMMA' in fmt and 'F16' in fmt
      function = runtime.fxn

      def execute(*arguments):
        pointers = dict(zip(program.arg.globals, (argument.value for argument in arguments), strict=True))
        vector = pointers[1]
        wave = self.waves[vector]
        scalar = pointers.get(0)
        mask = ctypes.c_uint32.from_address(scalar + EXEC_LO.offset * 4).value if scalar else (1 << wave['lanes']) - 1
        size = wave['wave_size'] * count * 4
        offset = register * wave['wave_size'] * 4
        before = ctypes.string_at(vector + offset, size)
        function(*arguments)
        self.executed += 1
        after = ctypes.string_at(vector + offset, size)
        words = struct.unpack(f'<{size // 4}I', after)
        failed = []
        for index, word in enumerate(words):
          lane = index % wave['wave_size']
          if not mask & (1 << lane):
            continue
          if 'F32' in fmt:
            invalid = word & 0x7FFFFFFF > 0x7F800000
          else:
            limit = 0x7F80 if 'BF16' in fmt else 0x7C00
            values = (word & 0xFFFF, word >> 16) if packed else ((word >> 16) if getattr(instruction, 'opsel', 0) & 8 else word & 0xFFFF,)
            invalid = any(value & 0x7FFF > limit for value in values)
          if invalid:
            failed.append({'register': register + index // wave['wave_size'], 'lane': lane, 'bits': f'{word:08x}'})
        if failed:
          self.capture(pc, instruction, wave, vector, scalar, before, after, failed, fmt)
          raise FirstNaN(f'kernel {self.boundary.kernels} {instruction!r} created NaN in active FP destination')

      return (program, SimpleNamespace(fxn=execute)), instruction

    emu._init_wave, emu._decode_at = init_wave, decode_at

  def capture(self, pc, instruction, wave, vector, scalar, before, after, failed, fmt):
    from test.mockgpu.amd.amdgpu import regCOMPUTE_USER_DATA_0

    self.evidence.mkdir(parents=True, exist_ok=True)
    (self.evidence / 'destination-before.bin').write_bytes(before)
    (self.evidence / 'destination-after.bin').write_bytes(after)
    (self.evidence / 'vgpr-after.bin').write_bytes(ctypes.string_at(vector, 256 * wave['wave_size'] * 4))
    if scalar:
      (self.evidence / 'sgpr-after.bin').write_bytes(ctypes.string_at(scalar, 260 * 4))
    (self.evidence / 'instruction.bin').write_bytes(instruction.to_bytes())
    gpu = self.boundary.gpu
    address = gpu.regs[regCOMPUTE_USER_DATA_0] | gpu.regs[regCOMPUTE_USER_DATA_0 + 1] << 32
    arguments = virtual_bytes(gpu, address, 128)
    (self.evidence / 'kernel-arguments.bin').write_bytes(arguments)
    views = []
    for index, (pointer,) in enumerate(struct.iter_unpack('<Q', arguments)):
      mapping = next(((base, size) for base, size in gpu.mapped_ranges if base <= pointer < base + size), None)
      if mapping is None:
        continue
      size = min(4096, mapping[0] + mapping[1] - pointer)
      data = virtual_bytes(gpu, pointer, size)
      (self.evidence / f'argument-{index}.bin').write_bytes(data)
      views.append({'argument': index, 'pointer': pointer, 'sample_bytes': size})
    result = {'kernel': self.boundary.kernels, 'instruction': repr(instruction), 'format': fmt,
      'pc': pc, 'program_offset': pc - wave['program'], 'wave': wave, 'nan_destinations': failed,
      'fp_instructions_executed': self.executed, 'dynamic_arguments': views, 'snapshot': self.snapshot,
      'scope': 'First active destination NaN among FP32/FP16/BF16 instructions; integer bits are not interpreted as float outputs.'}
    (self.evidence / 'first-nan.json').write_text(json.dumps(result, indent=2) + '\n')
