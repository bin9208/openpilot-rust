# Offline QCOM launch geometry (#282)

[Issue #282](https://github.com/bin9208/openpilot-rust/issues/282).
Base: `9200ffe57c385e2fa4a89ff11821c7e9ac710d2f`; branch:
`codex/fix-282-qcom-launch-geometry`. Implementation and host export checks
are complete locally; canonical model export, package integration and exact-SHA
CI remain the parent task's work. Device observations below concern isolated
probes with a private catalog, not activation of a rebuilt installed package.

## Cause and bounded change

`offline_schedule.py` lowers under `NOLOCALS=1`. For a program whose indexing
uses global work-item IDs, `ProgramInfo.local_size` is `None` and its global
dimensions count work-items. The exporter previously serialized those dimensions
with `(1,1,1)` local sizes. Original tinygrad
[`optimize_local_size`](../../tinygrad_repo/tinygrad/engine/realize.py)
benchmarks missing local sizes during execution and divides the work-item counts
by the selected local sizes before dispatch. Offline export does not execute
that optimizer.

The offline exporter now preserves every explicit local size and its existing
group dimensions. For missing locals, it selects power-of-two divisors in X,
Y, Z order with a total budget of 64 threads, then divides each work-item
dimension exactly. Dimensions must be positive integers within `u32`; the
algorithm introduces no padded work-items. Source containing group/local IDs,
group counts, local-size queries, barriers or local storage retains the old
single-thread groups.

Before regrouping, the exporter reads the compiled A630 register descriptor.
It rejects missing/out-of-bounds metadata or register counts outside the native
`ProgramImage` contract, and bounds the local product by original
[`QCOMProgram.max_threads`](../../tinygrad_repo/tinygrad/runtime/ops_qcom.py).
That formula is `min(1024, floor(384*32/(max(1,fregs+ceil(hregs/2))*128))*128)`.
Native-valid full/half register counts are at most 63 each, so even the largest
accepted pressure supports 128 threads. The selected 64-thread cap is lower.

Kernel source, compiled bytes, weights, buffer bindings, per-item arithmetic,
runtime priorities and validity rules are unchanged. Captured JIT export in
`qcom.py` retains the original runtime's already-selected geometry.

## Host evidence

Local artifacts are under `.omo/evidence/qcom-launch-282/`. Exact executable,
arguments and environment are recorded in the corresponding `*-invocation.json`.

| Scenario | Invocation / observable | Captured artifacts |
| --- | --- | --- |
| Real offline initialized arithmetic fixture, before the fix | `pytest test_offline_qcom_schedule.py`; 2 expected assertion failures: actual global `[4,1,1]`, expected `[1,1,1]` | `red-invocation.json`, `red.log`, `red.xml`, `red-fixtures/` |
| Missing and explicit locals, odd and multi-axis coverage, `u32` edges, invalid dimensions/registers, group-dependent fallback | `pytest test_offline_qcom_launch.py test_offline_qcom_schedule.py`; 35 passed, exact original work-item domain and correct interpreted values | `green-invocation.json`, `green.log`, `green.xml` |
| Full relevant offline/QCOM/CPU/pipeline export regression | `pytest` over 11 files, `DEV=CPU:LLVM`; 61 passed, including genuine ARM A630 compilation and trusted Rust bundle consumers | `regression-cpu-invocation.json`, `regression-cpu.log`, `regression-cpu.xml`, `regression-cpu-fixtures/` |
| Entire Rust tools test directory | `pytest rust/tools/tests`, matching CI `DEV=CPU:LLVM`, `JIT=2`, `CPU_COUNT=2`; 237 passed | `tools-suite-ci-invocation.json`, `tools-suite-ci.log`, `tools-suite-ci.xml`, `tools-suite-ci-fixtures/` |
| Source/binary/weight preservation | `PYTHONPATH=rust/tools:. python .omo/evidence/qcom-launch-282/verify_receipts.py`; both RED/GREEN bundle pairs byte-identical except call geometry, with exact per-axis work-item products | `verify_receipts.py`, `fixture-byte-comparison.json` |
| Manual driver probe policy comparison | Run `launch_dimensions` against existing original source/binaries, reconstructing `ProgramInfo` indexing from the original renderer; all call geometries match the manual probe, exactly 59 calls change, all source/binary/weight files byte-identical | `manual-probe-policy-comparison.json` |
| Static checks | Touched-file `ruff check`, programming rule audit, `git diff --check`; exit 0 | `checks.json`, `check-0.log`, `check-1.log`, `check-2.log` |

The first broader regression invocation auto-selected CUDA and failed eight
CPU-export tests because `nvcc` was absent. The complete same scenarios passed
after selecting `DEV=CPU:LLVM`; both runs are retained. LSP checking was unavailable
because basedpyright was previously declined; no dependency was installed.
The initial whole-tools run passed 235 tests and failed two NV12 warp capture
cases because the default JIT generated `Ops.CUSTOM_FUNCTION` instead of the
flat program contract. Rerunning the entire suite with the existing CI's `JIT=2`
and `CPU_COUNT=2` passed all 237 tests; both receipts are retained.

## Parent-executed physical probes

The parent changed only launch geometry in an isolated trusted driver graph.
An initial 12 uniform-frame A/B/A run took 4.218 / 0.503 / 4.198 seconds and
reported identical model output bytes. This was preliminary synthetic evidence.

A subsequent A/B/A replay used six real offroad 1928x1208 NV12 driver-camera
frames, each twice. The receipt checker directly compared every saved 553-float
model output and 1,382,400-byte warped output across the three variants, and
recomputed means from all 36 per-frame timing records:

| Variant | Copy mean ms | Prepare mean ms | Model mean ms | Total mean ms |
| --- | ---: | ---: | ---: | ---: |
| Original exported geometry | 0.955 | 59.229 | 265.224 | 325.408 |
| Grouped geometry | 0.892 | 1.512 | 16.440 | 18.845 |
| Original geometry restored | 0.924 | 59.217 | 265.813 | 325.954 |

Raw inputs/outputs and timing records remain private under
`.analysis/scratch/2026-10-13-c3x-boot/dm-camera-probe/` in the primary checkout.
The local verification artifact is `physical-replay-verification.json`.

The parent also ran the existing native driver daemon for 120 frames with the
private grouped catalog, normal camera IPC and message publication. Decoding the
captured full cereal messages directly verified 60 current-probe driver samples
and 60 driver-camera samples: all valid, consecutive frame IDs, mean driver
publication interval 50.00375 ms, maximum 51.37693 ms; model execution mean
20.00885 ms, range 19.16117–20.55944 ms. Camera publication mean was 50.00139 ms,
maximum 50.15564 ms. The probe exited 0. Parent restored the managed daemon and
reported `DisableDM=2` and preview disabled afterward. Source artifacts are
`dm-daemon-probe-result.json` and `dm-daemon-probe-queues.json` in the same private
primary scratch directory; independent decoded verification is
`physical-daemon-verification.json`.

These probes establish the isolated geometry defect and a private-catalog native
daemon's cadence. They do not establish a matched original-runtime performance
comparison, installed-package activation, loaded driving, thermal stability or
vehicle acceptance. A fresh full driver export must match the verified policy
before runtime packaging and handoff. Issue #282 stays open for its remaining
comparison/integration criteria. No user-setting behavior changes; user guides
are not needed for this exporter correction.
