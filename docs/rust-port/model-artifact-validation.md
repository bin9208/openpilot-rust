# Native CPU model artifact validation

Date: 2026-09-30. Scope: the CPU execution boundary within [#6](https://github.com/bin9208/openpilot-rust/issues/6).
Plan: [model-artifact-plan.md](model-artifact-plan.md). Full delivery gate:
[design.md](design.md). No production daemon has been switched by this increment.
This is not the full runtime port, a QCOM/AMD validation, or a device handoff.

## Execution boundary

`openpilot-model-runtime` owns aligned allocations, persistent state, aliased
views, named input/output copies and ordered kernel dispatch. `model-run` runs a
sequence of frames in one Rust process. The model library contains tinygrad's
compiled math kernels and fixed ABI wrappers. Python is used to capture/export
and produce reference outputs; it is not called by the Rust executor.

Both CPU Clang and LLVM artifacts are supported. The actual-model comparisons
use CPU LLVM, matching the original Linux CPU build selection. Each kernel is
compiled with the original tinygrad compiler through `compile_to_obj`; no
different math implementation, model, precision policy or error tolerance is
substituted. CPU worker partitions execute serially in this correctness runner.
That scheduling is not a production CPU performance measurement.

The loader validates manifest version/backend/architecture, ranges, cumulative
allocation and asset sizes, call arity, worker/core-id consistency, binding names
and SHA-256. It preserves shared allocations across views. The exporter also
checks captured input dtype/layout and parameter byte lengths before compiling.
Unsupported operations and symbolic scalars fail explicitly. Host-to-CPU copies
and constant host storage are represented in the exported graph.

Native library loading remains an unsafe trust boundary: trusted kernels must
match their metadata and finish synchronously without retaining pointers. Hashes
detect corruption and do not authenticate a publisher. Bundles must remain
immutable while loaded. These host artifacts must not be deployed to a vehicle.

## Observed host results

Seed: `20260930`. Rust 1.94.0, Python 3.12.14, NumPy 2.4.6, clang 18.1.3,
LLVM 20 (and a separate LLVM 18 DM comparison), `DEV=CPU:LLVM CPU_COUNT=4 JIT=2`.

| Comparison | Observed result |
| --- | --- |
| Driver-monitoring ONNX, 3 changed inputs | All 1,659 float32 outputs exactly equal; 85 calls, 44 kernels |
| Driving ONNX, 3 changed inputs using declared input dtypes | All 7,728 float32 outputs exactly equal; 154 calls, 71 kernels |
| Original recurrent policy, 128 frames with previous-feature feedback and history turnover | Model output and image/wide-image/feature/desire queues: 258,381,824 values exactly equal; 167 calls |
| Original road and DM camera preparation, 1928x1208 and 1344x760 | Four combinations pass; identity, projective/out-of-bounds and half-pixel transforms produce exactly equal pixels |
| Synthetic recurrent graph and overlapping output views | Four changed inputs exactly equal in a separate Rust process |
| Host-to-CPU copy and reordered keyword bindings | Native output exactly equals the source computation |

Model hashes:

- Driving: `f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5`.
- Driver monitoring: `dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04`.

The unchanged original policy feeds several float32 queues to ONNX inputs
declared float16; its original runner emits dtype warnings. The policy comparison
preserves that behavior. A separate direct-ONNX comparison uses the declared
dtypes. Neither result is a claim about QCOM `IMAGE=1 FLOAT16=1` arithmetic.

## Memory and rejection tests

- 13 Rust tests cover allocations, pointer lifetime across owner moves, aliasing,
  repeated state, independent instances, CLI multi-frame execution, corrupt
  assets, missing symbols, malformed ranges and invalid signatures/bindings.
- Eight Python tests exercise the actual exporter and original camera functions.
- The workspace test suite and workspace clippy passed locally. CI runs the
  inherited gates plus the new model comparisons and retains JSON reports.
- Miri's default x86 run passes. ARM Miri passes strict provenance, symbolic
  alignment and preemption under both Stacked Borrows and Tree Borrows.
- x86 Miri with symbolic alignment stops in dependency `memchr 2.8.3`'s SSE2
  aligned load during a JSON error path. The new allocation tests pass on x86;
  the full symbolic run is therefore recorded on ARM, not reported as x86 green.
- Actual Rust/C kernel integration passes ASan with `MODEL_TEST_ASAN=1` and
  `RUSTFLAGS='-Zsanitizer=address -Clink-arg=-Wl,--export-dynamic'`.

RED evidence preceded implementation: absent Rust Graph/CpuModel/CLI, unsupported
COPY calls, missing LLVM export support, undersized captured inputs accepted,
unreferenced weight bytes accepted, and out-of-limit weight offsets accepted.
The corresponding tests then passed after their implementations/fixes.

## Reproduction

From the repository root with the pinned Python dependencies available:

```sh
cd rust
cargo test --workspace --locked
cd ..
export PYTHONPATH=.:tinygrad_repo:rust/tools
export DEV=CPU:LLVM CPU_COUNT=4 JIT=2
export MODEL_RUN_BINARY="$PWD/rust/target/debug/model-run"
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=rust/tools/tests rust/tools/tests -q
python rust/tools/check_model_reference.py --model openpilot/selfdrive/modeld/models/dmonitoring_model.onnx --output /tmp/dm-native --binary "$MODEL_RUN_BINARY"
python rust/tools/check_model_reference.py --model openpilot/selfdrive/modeld/models/driving_supercombo.onnx --output /tmp/driving-native --binary "$MODEL_RUN_BINARY"
python rust/tools/check_policy_reference.py --model openpilot/selfdrive/modeld/models/driving_supercombo.onnx --output /tmp/policy-native --binary "$MODEL_RUN_BINARY"
```

The output directories must not already exist. Fetch the pinned driving model
with Git LFS from this independent repository before running its comparisons.
The policy check uses 128 frames so the 96/100-frame history queues turn over.
Its report records the actual compared count.

## Continuing GPU and full-runtime work

The official AGNOS compiler library was obtained from
[agnos-builder ec1cf237](https://github.com/commaai/agnos-builder/tree/ec1cf237a84565a056dbbc6f433b1b1c20c07a2c/userspace/root/usr/lib/aarch64-linux-gnu).
Its SHA-256 is `fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`.
An isolated ARM userspace under QEMU successfully compiled a small `a630` kernel
using the repository's QCOMCompiler. This proves offline compiler availability,
not GPU execution. The library stays outside Git.

Native QCOM parsing, arguments, KGSL ownership and dispatch now have a separate
[implementation ledger](qcom-runtime-validation.md); their host serialization
evidence does not establish actual-model GPU execution.

Actual QCOM GPU acceptance, AMD dispatch, VisionIPC, daemon integration, remaining M3-M6 ports,
normal startup/logging/upload comparison, and user device acceptance remain open.
No C3X connection, installation, reboot, or test was performed.
