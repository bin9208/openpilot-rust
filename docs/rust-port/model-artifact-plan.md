# Native model artifact implementation plan

Issue: #6, within the approved full runtime design (#1). This increment is not a
device-test handoff. Execution: native implementation, then independent review.

The build-time tinygrad exporter emits compiled kernels and a typed graph. Rust
owns allocations, aliased views, input updates, call order and persistent state.
Python remains an oracle/build dependency only. CPU artifacts establish the
execution contract; they do not replace the production QCOM or AMD backend.

## Contract

- Version 1 CPU bundle: `graph.json`, `weights.bin`, `kernels.so`. Manifest records
  backend `cpu-clang` or `cpu-llvm`, host architecture, SHA-256 of both assets, allocation byte
  lengths and weight offsets, buffer views, named input/output views, kernel
  signatures, and ordered calls (kernel index, view indices, scalar values,
  worker count and optional core-id scalar index).
- Views reference base allocations, never duplicate their storage. All ranges,
  indices, signature arities, dimensions and cumulative memory are checked before
  execution. Unsupported backend/version/architecture fails before loading code.
- `op_kernel_<index>(void **buffers, const int32_t *scalars)` is the generated C
  wrapper ABI. The original tinygrad compiler emits each kernel's object code;
  a small C wrapper calls it without changing its arithmetic. Rust dispatches ordered
  calls and each required CPU core-id. Serial worker execution is correct for
  independent CPU partitions; this is a correctness oracle, not a speed claim.
- Compiled kernels are trusted executable artifacts, like the existing model
  pickle. SHA-256 checks detect corruption, not publisher authenticity. The unsafe
  loading boundary documents that obligation. No arbitrary downloaded model is
  executed by default.

Implementation and observed results: [model-artifact-validation.md](model-artifact-validation.md).

## Task 1: graph validation

Files: `rust/crates/model-runtime/{Cargo.toml,src/lib.rs,src/graph.rs,tests/graph.rs}`;
workspace manifest and lockfile.

Interface: `Graph::parse(&[u8]) -> Result<ValidatedGraph, Error>`; serde structs
`Allocation`, `View`, `Binding`, `Kernel`, `Call`. Validated graph fields stay
private; inspection through immutable accessors only.

- Write failing tests for overlapping aliases, out-of-range/overflowed views,
  missing kernels, wrong call arity, duplicate bindings, incompatible versions,
  unsupported backends, excess memory and invalid worker/core-id combinations.
- Implement checked validation and typed diagnostics. Run the focused Rust tests.

## Task 2: native CPU execution

Files: `src/{buffer,cpu,error}.rs`, `src/bin/model-run.rs`, `tests/cpu.rs`.

Interface: unsafe `CpuModel::load(&Path) -> Result<CpuModel, Error>` for trusted
bundles, safe `write_input(&str, &[u8])`, `run()`, `read_output(&str, &mut [u8])`.
Allocation and native pointer handling live in a small private boundary. Buffer
reset is explicit; normal inference preserves state. No allocation in `run()`.

- Compile tiny C fixture kernels in tests. First reproduce missing runtime, then
  verify multiple recurrent steps, view aliasing, partitioned kernels, wrong
  input/output size, missing symbol, corrupt weights/library and independent model
  instances. CLI executes binary input/output files without Python.
- Run ordinary tests plus native sanitizer checks; Miri exercises allocation and
  view logic separately from unsupported dynamic-library calls.

## Task 3: actual tinygrad export and differential comparison

Files: `rust/tools/model_export/{__init__,capture,compile}.py`,
`rust/tools/tests/test_model_export.py`, `rust/tools/check_model_reference.py`.

Interface: `export_cpu(jit, bindings, destination)` consumes an already captured
JIT and named input/output tensors. Resolve PARAM slots, retain base-buffer
identity/byte offsets, flatten supported calls in execution order, and reject
unsupported operations or symbolic dimensions explicitly.

- First compare a captured recurrent graph and aliased outputs across multiple
  inputs with original tinygrad. Removing alias preservation or state retention
  must change the result. Use the Rust CLI as the separate-process executor.
- Export actual driver-monitoring and driving-model graphs from pinned model
  files; compare fixed-seed multi-input outputs, then frame preparation and
  recurrent policy queues. Record model hashes, build flags, maximum error and
  strict tolerance before accepting results. Do not substitute synthetic kernels
  for actual-model evidence.

## Continuing the approved full-runtime work

Once this contract is observed on CPU, implement native QCOM kernel descriptor,
argument packing, allocation, synchronization and submission, then the pinned
AMD/USB path. Keep backend tests and target builds distinct from hardware proof.
Continue M3-M6 and normal logging/upload comparison integration. None of these
CPU checks satisfies the user's full-conversion delivery gate.

## Review focus

1. Aliases and queue state survive repeated calls (Tasks 1-3).
2. Corrupt ranges and signatures fail before FFI (Tasks 1-2).
3. Export does not silently omit copy operations or symbolic bindings (Task 3).
4. Runtime contains no Python subprocess/import dependency (Tasks 2-3).
5. Evidence identifies CPU validation separately from actual GPU/device behavior.
