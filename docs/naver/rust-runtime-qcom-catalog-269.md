# Offline internal QCOM catalogs — issue #269

Issue: <https://github.com/bin9208/openpilot-rust/issues/269>. Recorded 2026-10-13.
This implements build-time generation from the exact original ONNX models. It
does not execute inference on an Adreno GPU or establish vehicle acceptance.

## Source and compilation contract

| Model | Exact raw ONNX SHA-256 | Bytes |
| --- | --- | ---: |
| Driver | `dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04` | 7,844,499 |
| Driving | `f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5` | 60,792,584 |

[offline_onnx.py](../../rust/tools/model_export/offline_onnx.py) verifies the raw
identity, uses the original `OnnxRunner`, reads actual initializer bytes on CPU,
and transfers only their descriptors to the lazy QCOM graph. Python shape
controls must derive from real initializers. Training, unsupported operators,
and runtime-content shape controls fail explicitly.

The original QCOM build flags in
[SConscript](../../openpilot/selfdrive/modeld/SConscript) remain `IMAGE=1`,
`FLOAT16=1`, `NOLOCALS=1`, `OPENPILOT_HACKS=1`; compilation uses `BEAM=0`.
Input promotion and float output conversion follow the original `compile3.py`.
[offline_pipeline.py](../../rust/tools/model_export/offline_pipeline.py) invokes
the unchanged `make_warp_dm`, `make_warp`, `make_run_policy`, and queue constructors.
It retains both original camera configurations, Venus NV12 layout, named state
buffers, and `prepare`/`model` or `prepare`/`policy` entrypoints. It uses the
original schedule-capture mechanism to collect runtime operations, without
inventing or evaluating runtime input/output values.

[offline_capability.py](../../rust/tools/model_export/offline_capability.py) and
[offline_schedule.py](../../rust/tools/model_export/offline_schedule.py) supply
QCOM's known offset capability during offline movement and memory planning.
`QCOMAllocator` inherits the actual `_offset` implementation from `HCQAllocatorBase`.
The original movement-view helper, pattern order, and TLSF algorithm remain in
use; both substitutions restore their original globals in `finally`. The owned
CPU control compares original and offline buffer lifetimes, offsets, and sizes.
No vendor source or GPU allocator is replaced.

[offline_constants.py](../../rust/tools/model_export/offline_constants.py) uses
the original `TinyJit.prune_linear` dependency rule. It executes the one-time
layout/copy ASTs on real CPU storage containing the actual initialized bytes.
Unknown reads and unaudited floating arithmetic fail. A nonzero half-precision
transpose/padding control checks exact resulting bytes. The DM model has 99
one-time transforms; the complete driving pipeline has 150. Runtime inference
is collected and compiled, never simulated with zeros or a NULL backend.

[offline_compiler.py](../../rust/tools/model_export/offline_compiler.py) invokes
the original A630 compiler under ARM Python/QEMU. The genuine `libllvm-qcom.so`
provider is 36,961,040 bytes, SHA-256
`fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`.
The target is `a630,IMAGE_PITCH_ALIGNMENT=64`. Compiler failures propagate;
global disk compiler caching is disabled. The production
[host-only guard](../../rust/tools/model_export/offline_devices.py) permits CPU,
NPY, Python, and file-backed DISK access and rejects GPU initialization before
opening a device. No device, NAS, or external recipient was contacted.

## Observed checks and artifact receipts

Private raw artifacts are under `.omo/evidence/269-qcom-catalog/`; retain their
concise receipts rather than committing binaries or raw captures.

| Scenario | Invocation / observable | Artifact |
| --- | --- | --- |
| Real small slice | Original lazy arithmetic, genuine A630 compiler, exact nonzero weights; ARM `qcom-model-run --check-bundle` exits 0 with `gpu_executed=false` | `vertical-slice/result.json` |
| Owned regressions | Scoped pytest with `PYTHONPATH=.:rust/tools:tinygrad_repo`, `DEV=CPU:LLVM`, retained LLVM provider and compiler argv: 13 tests pass; Ruff and no-excuse audit pass | `offline-final-checks.json`, `offline-final-{0,1,2}.log` |
| Complete TICI driver pipeline | 50 unique kernels, 97 calls; `prepare [0,2)`, `model [2,97)`; ARM checker exits 0 | `driver-pipeline-first/result.json` |
| Complete TICI driving pipeline | 89 unique kernels, 183 calls; `prepare [0,9)`, `policy [9,183)`; ARM checker exits 0 | `driving-pipeline-first/result.json` |
| Normal driver catalog CLI | Both camera configurations, generation `84fb7d325c16c987cd3e49b860acced4bbea6e8a6a3b4bde3480319eae058f46`; ARM `model-run --check-catalog` and both bundle checks exit 0 | `driver-catalog-build.json`, `driver-catalog-admission.json` |
| Normal driving catalog CLI | Both camera configurations, generation `c517e0c6646f5e8e8a35b86dcca0af69f776add37c4eda50b972baadf145c8b3`; unchanged producer snapshot; ARM catalog and both bundle checks exit 0 | `driving-catalog-build.json`, `driving-catalog-admission.json` |
| Corruption controls | Copies with corrupt index, pipeline, same-size weights, or same-size kernel are all rejected; staged catalog is preserved | `driver-catalog-corruption.json` |

The TICI driver weights are 12,026,368 bytes, SHA-256
`a3b185795cb051b01103ffc0ccb2c13881d5fa0dd973e8b3e17fd5d4f6cef4b7`.
The TICI driving weights are 67,077,565 bytes, SHA-256
`fc61cd336628e856311aef1858b7d8eb9e0dbe43b40d7c7fa1e5827c4d573b49`.
The corresponding admitted allocation totals are 26,510,548 and 82,723,323 bytes.
The numerical small-slice control executes original emitted low-level IR in the
original software interpreter. Only the interpreter's unsupported `idx0`
intrinsic is mapped to `g0`, with local size exactly 1; emitted GPU source/binary
remain unchanged. This is a software control, not GPU inference validation.

The source driving policy constructs `packed_npy_inputs`, `feat_q`, and `desire_q`
as float32. Its unchanged split/sample operations feed those tensors to an ONNX
spec declaring half inputs. Original `OnnxRunner._parse_input` warns and retains
the supplied Tensor dtype. The recorded desire/traffic/action/features warnings
therefore follow the original policy path. No cast or warning suppression is
added. Source flags, source functions and producer SHA snapshots are retained.

## Remaining acceptance limits

All four exact driver/driving camera variants are generated and admitted. Admission
checks parse actual binaries, weights, views, images and entrypoints; they do not
submit GPU work. Physical KGSL allocation/import, GPU execution, numerical model
outputs, recurrent-state behavior on the GPU, real-time performance, and full
vehicle startup/drive comparison remain unvalidated. Python LSP was unavailable;
no type-checker success is claimed. No device-test handoff or CPU-saving claim is
made by this component receipt.
