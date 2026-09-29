from __future__ import annotations

from dataclasses import dataclass

from tinygrad.runtime.support.compiler_cpu import ClangCompiler, CPULLVMCompiler
from tinygrad.uop.ops import UOp

from .schema import ExportError, Kernel


@dataclass(frozen=True, slots=True)
class NativeKernel:
    signature: Kernel
    source: str
    object_code: bytes
    wrapper: str


def compile_kernel(operation: UOp, index: int, compiler: ClangCompiler | CPULLVMCompiler) -> NativeKernel:
    info = operation.arg
    source = operation.src[3].arg
    if not isinstance(source, str):
        raise ExportError("compiled program has no source")
    symbol = f"op_impl_{index}"
    original = f"void {info.function_name}(" if isinstance(compiler, ClangCompiler) else f"@{info.function_name}("
    renamed = f"void {symbol}(" if isinstance(compiler, ClangCompiler) else f"@{symbol}("
    if source.count(original) != 1:
        raise ExportError("unrecognized kernel declaration")
    body = source.replace(original, renamed)
    kernel = Kernel(len(info.globals), len(info.vars))
    arguments = [f"buffers[{i}]" for i in range(kernel.buffers)] + [f"scalars[{i}]" for i in range(kernel.scalars)]
    types = ["void *"] * kernel.buffers + ["int32_t"] * kernel.scalars
    wrapper = (f"extern void {symbol}({', '.join(types) or 'void'});\n" +
               f"void op_kernel_{index}(void **buffers, const int32_t *scalars) {{ {symbol}({', '.join(arguments)}); }}")
    return NativeKernel(kernel, source, compiler.compile_to_obj(body), wrapper)
