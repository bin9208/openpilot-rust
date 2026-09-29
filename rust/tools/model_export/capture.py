from __future__ import annotations

from tinygrad import TinyJit
from tinygrad.engine.jit import _prepare_jit_inputs
from tinygrad.device import Device
from tinygrad.engine.realize import resolve_params
from tinygrad.renderer.cstyle import ClangRenderer
from tinygrad.renderer.llvmir import CPULLVMRenderer
from tinygrad.runtime.support.compiler_cpu import ClangCompiler, CPULLVMCompiler
from tinygrad.uop.ops import Ops

from .buffers import BufferTable, host_buffer
from .native import compile_kernel
from .schema import Binding, Bindings, Call, CapturedModel, ExportError, Kernel


def capture_cpu(jit: TinyJit, bindings: Bindings) -> CapturedModel:
    captured = jit.captured
    if captured is None:
        raise ExportError("JIT must be captured before export")
    renderer = Device.default.renderer
    if not isinstance(renderer, (ClangRenderer, CPULLVMRenderer)) or not isinstance(renderer.compiler, (ClangCompiler, CPULLVMCompiler)):
        raise ExportError("export requires DEV=CPU:CLANG or CPU:LLVM")
    if len(captured.expected_names) != len(captured.expected_input_info) or len(bindings.inputs) != len(captured.expected_names):
        raise ExportError("export requires explicit flat input bindings for every captured input")
    names = list(bindings.inputs)
    ordered = [bindings.inputs[name] if isinstance(name, str) else bindings.inputs[names[name]] for name in captured.expected_names]
    input_uops, variables, _, input_info = _prepare_jit_inputs(tuple(ordered), {})
    if variables or input_info != captured.expected_input_info:
        raise ExportError("bindings disagree with the captured input contract")
    inputs = tuple(input_uops)
    for call in captured.linear.src:
        for argument in call.src[1:]:
            for parameter in argument.toposort():
                if parameter.op is Ops.PARAM:
                    buffer = host_buffer(inputs[parameter.arg.slot].buffer)
                    if parameter.max_numel() * parameter.dtype.itemsize != buffer.nbytes:
                        raise ExportError("buffer size disagrees with the captured input contract")
    table = BufferTable()
    input_bindings = []
    for name, tensor in bindings.inputs.items():
        buffer = host_buffer(tensor.uop.base.buffer)
        if buffer.nbytes != tensor.numel() * tensor.dtype.itemsize:
            raise ExportError("input bindings must cover their complete base buffer")
        table.read(buffer)
        input_bindings.append(Binding(name, table.view(buffer)))
    output_bindings = []
    for name, tensor in bindings.outputs.items():
        buffer = host_buffer(tensor.uop.buffer)
        table.read(buffer)
        output_bindings.append(Binding(name, table.view(buffer)))
    sources: list[str] = []
    objects: list[bytes] = []
    wrappers: list[str] = []
    kernels: list[Kernel] = []
    calls: list[Call] = []
    kernel_ids: dict[str, int] = {}
    for call in captured.linear.src:
        if call.op is not Ops.CALL:
            raise ExportError(f"unsupported captured operation: {call.op}")
        operation = call.src[0]
        buffers = [host_buffer(u.buffer) for u in resolve_params(call, inputs)]
        if operation.op is Ops.SLICE:
            if len(buffers) != 2 or buffers[0].base is not buffers[1].base:
                raise ExportError("slice must preserve backing allocation")
            table.view(buffers[0])
            continue
        if operation.op is Ops.COPY:
            if len(buffers) != 2 or buffers[0].nbytes != buffers[1].nbytes:
                raise ExportError("copy requires equally sized source and destination")
            key = f"copy:{buffers[0].nbytes}"
            if key not in kernel_ids:
                index = len(kernels)
                kernel_ids[key] = index
                kernels.append(Kernel(2, 0))
                sources.append(key)
                objects.append(b"")
                wrappers.append(f"void op_kernel_{index}(void **buffers, const int32_t *scalars) {{ memmove(buffers[0], buffers[1], {buffers[0].nbytes}); }}")
            table.read(buffers[1])
            table.write(buffers[0])
            calls.append(Call(kernel_ids[key], tuple(table.view(buffer) for buffer in buffers), (), 1, None))
            continue
        if operation.op is not Ops.PROGRAM:
            raise ExportError(f"unsupported captured call: {operation.op}")
        info = operation.arg
        if any(buffers[index].device.split(":")[0] != "CPU" for index in info.globals):
            raise ExportError("compiled kernel requires CPU storage")
        if any(variable.expr != "core_id" for variable in info.vars):
            raise ExportError("symbolic kernel scalars require an explicit runtime contract")
        if any(not isinstance(dim, int) for dim in info.global_size) or tuple(info.global_size[1:]) != (1, 1):
            raise ExportError("unsupported CPU launch dimensions")
        if info.local_size is not None and tuple(info.local_size) != (1, 1, 1):
            raise ExportError("unsupported CPU local dimensions")
        source = operation.src[3].arg
        if not isinstance(source, str):
            raise ExportError("compiled program has no C source")
        if source not in kernel_ids:
            index = len(kernels)
            kernel_ids[source] = index
            native = compile_kernel(operation, index, renderer.compiler)
            kernels.append(native.signature)
            sources.append(native.source)
            objects.append(native.object_code)
            wrappers.append(native.wrapper)
        for index in info.ins:
            table.read(buffers[index])
        for index in info.outs:
            table.write(buffers[index])
        calls.append(Call(kernel_ids[source], tuple(table.view(buffers[i]) for i in info.globals), tuple(0 for _ in info.vars),
                          info.global_size[0], info.runtimevars.get("core_id")))
    allocations, weights = table.weights()
    return CapturedModel(allocations, tuple(table.views), tuple(input_bindings), tuple(output_bindings), tuple(kernels), tuple(calls),
                         tuple(sources), tuple(objects), tuple(wrappers), weights,
                         "cpu-clang" if isinstance(renderer.compiler, ClangCompiler) else "cpu-llvm")
