"""Export captured AMD programs as verified ELF/data, without runtime Python."""

from __future__ import annotations

import hashlib
import json
import math
from dataclasses import asdict, dataclass
from pathlib import Path

from tinygrad.device import Buffer, Device, MultiBuffer
from tinygrad.engine.jit import _prepare_jit_inputs
from tinygrad.engine.realize import resolve_params
from tinygrad.renderer.llvmir import AMDLLVMRenderer
from tinygrad.uop.ops import Ops

from .binding_order import ordered_inputs
from .buffers import BufferTable
from .pipeline import Pipeline
from .qcom import CopyCall, KernelCall
from .schema import Allocation, Binding, Bindings, Entrypoint, ExportError, View, entrypoint_version


@dataclass(frozen=True, slots=True)
class Kernel:
  name: str
  binary_sha256: str
  binary_bytes: int


@dataclass(frozen=True, slots=True)
class Manifest:
  version: int
  backend: str
  arch: str
  weights_sha256: str
  allocations: tuple[Allocation, ...]
  views: tuple[View, ...]
  inputs: tuple[Binding, ...]
  outputs: tuple[Binding, ...]
  kernels: tuple[Kernel, ...]
  calls: tuple[KernelCall | CopyCall, ...]
  entrypoints: tuple[Entrypoint, ...]


def amd_buffer(buffer: Buffer | MultiBuffer) -> Buffer:
  match buffer:  # MATCH_OK: External buffers and unsupported storage are rejected at this export boundary.
    case Buffer() if buffer.device.split(':')[0] in {'AMD', 'NULL', 'CPU', 'NPY'}:
      return buffer
    case _:
      raise ExportError('AMD export requires one owned AMD or host buffer')


def export_amd(pipeline: Pipeline, bindings: Bindings, destination: Path) -> None:
  captured = pipeline.jit.captured
  if captured is None:
    raise ExportError('AMD graph must be captured before export')
  input_uops, variables, _, info = _prepare_jit_inputs(ordered_inputs(captured.expected_names, bindings.inputs), {})
  if variables or info != captured.expected_input_info:
    raise ExportError('AMD bindings disagree with captured inputs')
  table = BufferTable()
  inputs, outputs = [], []
  for names, target in ((bindings.inputs, inputs), (bindings.outputs, outputs)):
    for name, tensor in names.items():
      buffer = amd_buffer(tensor.uop.buffer)
      if buffer.nbytes != tensor.numel() * tensor.dtype.itemsize:
        raise ExportError('AMD binding must cover the complete tensor view')
      table.read(buffer)
      target.append(Binding(name, table.view(buffer)))
  kernels: list[Kernel] = []
  binaries: list[bytes] = []
  calls: list[KernelCall | CopyCall] = []
  kernel_ids: dict[Kernel, int] = {}
  for call in captured.linear.src:
    if call.op is not Ops.CALL:
      raise ExportError('AMD graph requires captured calls')
    operation = call.src[0]
    buffers = [amd_buffer(uop.buffer) for uop in resolve_params(call, tuple(input_uops))]
    match operation.op:  # MATCH_OK: Tinygrad's open operation set has an explicit unsupported-operation error.
      case Ops.SLICE:
        if len(buffers) != 2 or buffers[0].base is not buffers[1].base:
          raise ExportError('AMD slice must preserve its backing allocation')
        table.view(buffers[0])
        continue
      case Ops.COPY:
        if len(buffers) != 2 or buffers[0].nbytes != buffers[1].nbytes:
          raise ExportError('AMD copy requires equally sized views')
        table.read(buffers[1])
        table.write(buffers[0])
        calls.append(CopyCall(table.view(buffers[1]), table.view(buffers[0])))
        continue
      case Ops.PROGRAM:
        pass
      case _:
        raise ExportError('unsupported captured AMD operation')
    program = operation.arg
    renderer = Device[operation.src[1].arg].renderer
    if not isinstance(renderer, AMDLLVMRenderer) or renderer.target.arch != 'gfx1200':
      raise ExportError('AMD LLVM gfx1200 compiled program required')
    if any(buffers[index].device.split(':')[0] not in {'AMD', 'NULL'} for index in program.globals):
      raise ExportError('AMD program requires AMD device storage')
    local = program.local_size or (1, 1, 1)
    if program.vars or len(local) != 3 or len(program.global_size) != 3:
      raise ExportError('static AMD launch dimensions required')
    if any(type(dim) is not int or dim <= 0 for dim in local) or any(not math.isfinite(dim) or dim <= 0 or int(dim) != dim for dim in program.global_size):
      raise ExportError('positive integer AMD launch dimensions required')
    binary = operation.src[4].arg
    if not isinstance(binary, bytes) or not binary.startswith(b'\x7fELF'):
      raise ExportError('AMD compiled ELF bytes missing')
    kernel = Kernel(program.function_name, hashlib.sha256(binary).hexdigest(), len(binary))
    if kernel not in kernel_ids:
      kernel_ids[kernel] = len(kernels)
      kernels.append(kernel)
      binaries.append(binary)
    for index in program.ins:
      table.read(buffers[index])
    for index in program.outs:
      table.write(buffers[index])
    calls.append(
      KernelCall(kernel_ids[kernel], tuple(table.view(buffers[index]) for index in program.globals), (), tuple(int(dim) for dim in program.global_size), local)
    )
  allocations, weights = table.weights()
  version = entrypoint_version(pipeline.entrypoints, len(calls))
  manifest = Manifest(
    version,
    'amd-elf',
    'gfx1200',
    hashlib.sha256(weights).hexdigest(),
    allocations,
    tuple(table.views),
    tuple(inputs),
    tuple(outputs),
    tuple(kernels),
    tuple(calls),
    pipeline.entrypoints,
  )
  serialized = asdict(manifest)
  for call in serialized['calls']:
    if 'global_' in call:
      call['global'] = call.pop('global_')
  destination.mkdir(parents=True, exist_ok=False)
  for index, binary in enumerate(binaries):
    (destination / f'kernel-{index}.bin').write_bytes(binary)
  (destination / 'weights.bin').write_bytes(weights)
  (destination / 'graph.json').write_text(json.dumps(serialized, indent=2) + '\n')
