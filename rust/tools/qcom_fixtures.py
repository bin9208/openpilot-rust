# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Build QCOMCL oracle fixtures without opening a GPU device."""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import itertools
import json
import os
import tempfile
from pathlib import Path
from types import SimpleNamespace

from tinygrad.dtype import dtypes
from tinygrad.runtime.ops_qcom import QCOMArgsState, QCOMComputeQueue, QCOMProgram
from tinygrad.runtime.autogen import kgsl
from tinygrad.runtime.support.compiler_qcom import QCOMCompiler
from tinygrad.runtime.support.hcq import HCQBuffer, MMIOInterface

KERNELS = {
  "buffer_add": "__kernel void buffer_add(__global float *out, __global const float *inp) { int i=get_global_id(0); out[i]=inp[i]+1.0f; }",
  "scalar_add": "__kernel void scalar_add(__global float *out, __global const float *inp, int offset) { int i=get_global_id(0); out[i]=inp[i]+offset; }",
  "image_copy": ("__kernel void image_copy(write_only image2d_t out, read_only image2d_t inp) { int2 p=(int2)(get_global_id(0),get_global_id(1)); " +
                 "const sampler_t s=CLK_NORMALIZED_COORDS_FALSE|CLK_ADDRESS_CLAMP|CLK_FILTER_NEAREST; write_imagef(out,p,read_imagef(inp,s,p)); }"),
  "constants": ("__constant float coeffs[4]={1.25f,2.5f,3.75f,4.0f}; __kernel void constants(__global float *out, __global const float *inp) " +
                "{ int i=get_global_id(0); out[i]=inp[i]*coeffs[i&3]+0.75f; }"),
}
FIELDS = ("image_size", "prg_offset", "brnchstck", "pvtmem", "shmem", "samp_cnt", "samplers", "buf_offs", "tex_cnt", "ibo_cnt",
          "ibo_off", "tex_off", "samp_off", "consts_info", "fregs", "hregs", "pvtmem_size_per_item", "pvtmem_size_total",
          "hw_stack_offset", "shared_size", "max_threads", "kernargs_alloc_size")
ADDRESSES = {"program": 0x123400000, "stack": 0x234500000, "border": 0x345600000, "dummy": 0x456700000,
             "args": 0x567800000, "buffers": [0x678900000, 0x789A00000]}


class HostAllocator:
  def __init__(self):
    self.allocations = {}

  def alloc(self, size, _spec=None):
    backing = ctypes.create_string_buffer(size)
    address = ctypes.addressof(backing)
    result = HCQBuffer(address, size, view=MMIOInterface(address, size))
    self.allocations[id(result)] = backing
    return result

  def free(self, buffer, _size, _spec):
    del self.allocations[id(buffer)]


def generate(destination: Path, existing: Path | None = None):
  destination.mkdir(parents=True, exist_ok=False)
  compiler = QCOMCompiler("a630") if existing is None else None
  pinned_hash = "fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093"
  if compiler is not None and hashlib.sha256(Path(os.environ["LLVM_QCOM_PATH"]).read_bytes()).hexdigest() != pinned_hash:
    raise RuntimeError("QCOM fixture compiler differs from the pinned AGNOS library")
  provenance = {"compiler_sha256": pinned_hash,
                "reference": "tinygrad_repo/tinygrad/runtime/ops_qcom.py", "architecture": "a630"}
  (destination / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
  names = ["devinfo", "device_getproperty", "device_waittimestamp_ctxtid", "drawctxt_create", "drawctxt_destroy",
           "gpuobj_alloc", "gpuobj_free", "command_object", "gpu_command"]
  abi = {"structs": {name: {"size": ctypes.sizeof(typ := getattr(kgsl, "struct_kgsl_" + name)),
                           "fields": {field: getattr(typ, field).offset for field in typ.__annotations__}} for name in names},
         "ioctls": {name: (direction << 30 | ctypes.sizeof(typ) << 16 | kind << 8 | number)
                    for name in ["DEVICE_GETPROPERTY", "DEVICE_WAITTIMESTAMP_CTXTID", "DRAWCTXT_CREATE", "DRAWCTXT_DESTROY",
                                 "GPUOBJ_ALLOC", "GPUOBJ_FREE", "GPU_COMMAND", "SETPROPERTY"]
                    for direction, kind, number, typ in [getattr(kgsl, "IOCTL_KGSL_" + name).args]}}
  (destination / "kgsl.json").write_text(json.dumps(abi, indent=2) + "\n")
  for name, source in KERNELS.items():
    binary = compiler.compile(source) if compiler is not None else (existing / f"{name}.bin").read_bytes()
    (destination / f"{name}.cl").write_text(source + "\n")
    (destination / f"{name}.bin").write_bytes(binary)
    allocator = HostAllocator()
    stack_requests = []
    dev = SimpleNamespace(allocator=allocator, renderer=None, prof_prg_counter=itertools.count(), gpu_id=(6, 3, 0),
                          _ensure_stack_size=stack_requests.append, _stack=HCQBuffer(ADDRESSES["stack"], 1 << 20),
                          border_color_buf=HCQBuffer(ADDRESSES["border"], 4096), dummy_addr=ADDRESSES["dummy"])
    variants = [("f32", dtypes.imagef((64, 64, 4))), ("f16", dtypes.imageh((64, 64, 4)))] if name == "image_copy" else [("buffer", dtypes.float32)]
    record = None
    for variant, dtype in variants:
      program = QCOMProgram(dev, name, binary, buf_dtypes=[[(0, dtype)], [(0, dtype)]])
      program.lib_gpu.va_addr = ADDRESSES["program"]
      argbuf = allocator.alloc(program.kernargs_alloc_size)
      arg_address = argbuf.va_addr
      scalars = (7,) if name == "scalar_add" else ()
      state = QCOMArgsState(argbuf, program, tuple(HCQBuffer(address, 65536) for address in ADDRESSES["buffers"]), scalars)
      argbuf.va_addr = ADDRESSES["args"]
      queue = QCOMComputeQueue(dev)
      queue.exec(program, state, (3, 5, 2), (8, 4, 1))
      (destination / f"{name}-{variant}.args").write_bytes(ctypes.string_at(arg_address, program.kernargs_alloc_size))
      words = list(queue._q)
      queue.memory_barrier()
      fractional = QCOMComputeQueue(dev).exec(program, state, (2.5, 3.25, 1), (8, 4, 1))
      record = {field: getattr(program, field) for field in FIELDS}
      record.update(binary_sha256=hashlib.sha256(binary).hexdigest(), image_sha256=hashlib.sha256(program.image).hexdigest(),
                    addresses=ADDRESSES, global_size=[3, 5, 2], local_size=[8, 4, 1], queue=words,
                    barrier=list(queue._q)[len(words):], stack_request=stack_requests[-1], fractional_queue=list(fractional._q))
      allocator.free(argbuf, argbuf.size, None)
    (destination / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n")
    print(name, len(binary), len(record["queue"]), flush=True)


def main():
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("destination", type=Path)
  parser.add_argument("--verify", action="store_true", help="recompute original metadata, arguments, packets and KGSL ABI on the host")
  args = parser.parse_args()
  if not args.verify:
    generate(args.destination)
    return
  with tempfile.TemporaryDirectory(prefix="qcom-oracle-") as temporary:
    destination = Path(temporary) / "fixtures"
    generate(destination, args.destination)
    mismatches = [path.name for path in destination.iterdir() if path.read_bytes() != (args.destination / path.name).read_bytes()]
    if mismatches:
      raise RuntimeError(f"QCOM oracle fixture mismatch: {mismatches}")
  print("All QCOM oracle fixtures match the original implementation.", flush=True)


if __name__ == "__main__":
  main()
