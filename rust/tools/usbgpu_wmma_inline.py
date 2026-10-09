from __future__ import annotations
import hashlib
import inspect


SOURCE_SHA256 = '91035902e6cbe7992422900242875dee9042801aa3d8f092fe627f4bca902dd6'


def install():
  from test.mockgpu.amd import emu

  original = emu._compile_wmma
  if hashlib.sha256(inspect.getsource(original).encode()).hexdigest() != SOURCE_SHA256:
    raise RuntimeError('owned WMMA operand adapter requires the recorded emulator compiler revision')

  def compile_wmma(instruction, context):
    if 'F32_16X16X16_F16' not in emu._op_name(instruction) and 'F32_16X16X16_BF16' not in emu._op_name(instruction):
      return original(instruction, context)
    read = context.rvgpr_dyn
    source = context.inst_field(type(instruction).src2)
    vector = source >= emu._c(256)
    reads = 0

    class Context:
      def __getattr__(self, name):
        return getattr(context, name)

      def rvgpr_dyn(self, register, lane, valid=None):
        nonlocal reads
        reads += 1
        # The hash-bound compiler reads 256 A, 256 B, then 256 C elements.
        if reads <= 512:
          return read(register, lane, valid)
        offset = vector.where(register + emu._c(256), source)
        return context.rsrc_dyn(offset, lane)

    result = original(instruction, Context())
    if reads != 768:
      raise RuntimeError(f'owned WMMA operand adapter saw {reads} register reads, expected 768')
    return result

  emu._compile_wmma = compile_wmma
  emu._get_runner.cache_clear()
  emu._canonical_runner_cache.clear()
