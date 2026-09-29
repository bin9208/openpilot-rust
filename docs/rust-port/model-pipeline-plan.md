# Native model pipeline and build integration

Issue #12 builds on the CPU/QCOM executors in #6. The whole-runtime gate in #1
remains active: no first device-test handoff before the project-owned runtime
can start, log and use the existing upload comparison flow.

## Grounded behavior

The original driving artifact contains metadata, a `run_policy` TinyJit and one
warp TinyJit per camera resolution. `ModelState.run` warps first and returns on
`prepare_only` before shifting policy queues. Driver monitoring similarly passes
the warp tensor directly to its inference JIT. These buffers share the original
device; separate native model contexts would add avoidable image copies.

The original build scripts already compile and validate target model artifacts.
Keep Python as a build/oracle dependency and convert these trusted artifacts.
Do not invent a fake GPU execution result or a host timing-based launch policy.
Host validation uses actual CPU artifacts produced by those same build scripts.

## Implementation

1. Add version-2 entrypoint ranges to native CPU/QCOM graphs. Ranges form a
   named, ordered partition of calls; reject invalid ranges, duplicates and
   inconsistent versions. Version-1 single-entry bundles remain supported.
   Add `run_entry` and an optional CLI frame entrypoint; `run` still executes all.
2. Preserve shared allocation ownership. CPU entrypoints select invocation
   ranges. QCOM prepares per-call arguments once, folds full/named command
   batches at build/load time, and reuses buffers during inference. CPU copies
   retain their original ordering and synchronization boundaries.
3. Reconstruct captured inputs from both expected view metadata and PARAM base
   sizes. NOOP views do not have a usable shape by themselves. Create a fresh
   base tensor and substitute it into the recorded view, then verify the full
   original input contract. Reject dynamic, conflicting or missing contracts.
4. Compose original warp/policy or warp/DM graphs by resolving each stage's
   parameters to concrete buffers. Reuse the warp output tensor as the next
   stage input. Feed the combined graph and external bindings through the
   existing exporters so one buffer table preserves cross-stage aliases.
   Derive entrypoint ranges from emitted PROGRAM/COPY calls, excluding SLICE
   metadata operations, and check the resulting call count.
5. Emit model metadata and a pipeline index suitable for Rust consumers, with
   source/model hashes and bounded schema validation. Keep trusted-pickle
   conversion explicit. Publish generated bundles atomically; do not overwrite
   artifacts loaded by a running model.
6. Add an explicit build target for native artifacts after original compilation.
   Do not select a production Rust daemon before the remaining runtime exists.
   Record QCOM progress in the port manifest and keep device acceptance separate.

## Verification

Write failing tests for named stages, invalid contracts, shared buffers and
prepare-only state behavior before implementation. Compare real original CPU
build artifacts with the Rust process for road and driver pipelines, both camera
resolutions and recurrent queue turnover. Preserve actual model dtypes, output
slices and feature feedback; do not loosen numeric tolerances.

The initial policy import prototype compared model output and four state queues:
6,055,824 values over three frames matched exactly. It is grounding evidence,
not a substitute for the integrated pipeline checks. GPU numerics, KGSL behavior,
AMD/USB, VisionIPC and full daemon startup remain separately tracked work.

Run relevant host checks, independent review, required exact-head CI and
post-merge checks. No vehicle connection, installation, reboot or test is part
of this implementation increment.

## Implementation and host evidence (2026-09-30)

The native executors now support named entrypoints over one allocation table.
CPU selects preloaded calls; QCOM prepares full and named command batches once,
including the original synchronization boundaries around host copies. A host
driver test checks stage selection, skipped copies, unknown/empty stages and
that repeated execution makes no new driver allocations. This does not execute
a GPU kernel.

The converter reads explicitly trusted original compiled artifacts. It resolves
recorded PARAM sizes and erased input views, preserves shared warp output and
state storage, and expands the original grouped JIT calls without recompiling
or simulating their GPU behavior. Both grouped and ungrouped CPU captures pass
the native staged-state regression.

`scons native_models` adds an explicit conversion target after the existing
driving, driver, metadata and camera-warp build outputs. Normal SCons targets do
not select a Rust daemon. Generated data goes under `rust/target/native-models`.
Each generation contains graph and pipeline descriptors, source artifact hashes,
camera layout, tensor contracts and normalized output slices. Publication writes
an immutable generation before atomically replacing `current.json`. A failed
conversion or corrupt existing generation leaves the previous pointer intact.
The Rust catalog reader checks descriptor hashes, graph bindings and lengths,
camera layout, model output shape, output slices and entrypoint identity.

Real original CPU artifacts were converted and compared through the Rust process:

| Pipeline | Camera | Frames | Prepare-only | Compared values | Result |
| --- | --- | ---: | ---: | ---: | --- |
| Driving | 1928x1208 | 128 | 12 | 308,713,472 | Identical output bytes |
| Driving | 1344x760 | 128 | 12 | 308,713,472 | Identical output bytes |
| Driver monitoring | 1928x1208 | 3 | 0 | 4,148,859 | Identical output bytes |
| Driver monitoring | 1344x760 | 3 | 0 | 4,148,859 | Identical output bytes |

Driving comparisons include warped images, model output, image/feature/desire
queues and hidden-feature feedback through queue turnover. The generated
four-bundle catalog was read successfully with `model-run --check-catalog`.
The repeatable CI pipeline also builds grouped artifacts using the original
compiler scripts and runs the comparisons above. Exact-head CI, review and
post-merge results are recorded in issue #12; pending runs are not passes.

Python remains a build and reference dependency. The Rust native executors do
not import or launch Python. AMD/USB execution, VisionIPC, model preprocessing
outside these captured graphs, output interpretation/publication and the other
production services remain required by the full-runtime gate in #1. These host
results establish neither GPU/device numerics nor a CPU reduction in a vehicle.
