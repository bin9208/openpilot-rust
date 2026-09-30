# Rust torque estimator and continuous daemon

Issue [#40](https://github.com/bin9208/openpilot-rust/issues/40) is an intermediate
part of [#1](https://github.com/bin9208/openpilot-rust/issues/1). The source base is
`6836b9cf89b42d407d0196d86bb9f6daad719636`. This increment does not select a new
production process, connect to a vehicle, or meet the complete runtime/device
comparison gate. The original `torqued.py`, `locationd/helpers.py`, cereal
schemas, repository-locked NumPy 2.5.3 and original messaging classes are the current validation oracles.
Original files and licensing remain; additional NumPy notices are retained in
`rust/crates/torqued/NOTICE.md`.

## Source-lock correction

The initial `4e9bd7eccaa5c97c58c62f5f5480bcd15ba6a73f` increment used
NumPy 2.4.6/OpenBLAS 0.3.31 even though `pyproject.toml` requires NumPy>=2.5.3 and
`uv.lock` pins 2.5.3. That initial dependency was not suitable as the source-locked
runtime candidate. Current staging requires the exact archive filename and
SHA256 from `uv.lock`, and the Rust loader requires NumPy 2.5.3 and its
OpenBLAS 0.3.34.106.0 ILP64 ABI. Earlier 2.4.6 evidence is retained and explicitly
historical below; it does not establish 2.5.3 compatibility.

All NumPy installs in the Rust workflow and eleven standalone oracle/build script pins now
use 2.5.3. This is a shared validation prerequisite: existing captured model and
tinygrad pipeline artifacts must be rebuilt/revalidated by the exact-head CI
jobs under 2.5.3. Earlier 2.4.6 model/pipeline results do not cover this dependency
change. The pipeline descriptors currently record model/source hashes but not the
NumPy version, so a previously generated artifact alone cannot prove this
prerequisite. Local torque validation uses new isolated host/ARM environments; shared
MODEL/LOG environments are unchanged.

## Runtime behavior

`openpilot-torqued` waits for nonempty `CarParams` through the shared runtime
Params paths. It restores the fingerprint/tuning/friction/factor/version cache
identity, including source partial-assignment behavior before malformed cache
removal. The original `useParams` brand policy, offline parameter bounds,
Float32-to-Float64 promotion, 100-sample independent histories, lag handling,
calibration rotation, device roll, engagement/override interpolation, admission
thresholds, eight FIFO buckets, decimated analysis mode, fit counts, progress,
filter alpha ordering, clipping and reset semantics are preserved.

The RNG implements the original MT19937 scalar-seed stream, rejection interval
selection and full permutation-prefix `choice(..., replace=False)`. Even fitting
all available points consumes a permutation. Production initializes MT19937
from fresh OS entropy; there is no production fixed seed or Python process.
Deterministic seeds exist only in Rust library test/examples for source coupling.
The exact OS-generated initial stream is intentionally not an equivalence claim.

The daemon polls `livePose`, subscribes in original order to `carControl`,
`carOutput`, `carState`, `liveCalibration`, `livePose`, `liveDelay`, and feeds
updates only when `all_checks()` passes. Every update uses the source 100 ms
poll timeout. Publication occurs at `frame % 5 == 0`; persistence at
`frame % 240 == 0` performs a second independent fit/filter update with all
points. The source comment says sixty seconds, but its actual modulus is
preserved. Startup frame zero publishes and persists. Cache writes run on a
FIFO worker thread; shutdown drains pending writes. A held filesystem lock can
therefore delay orderly exit while publication remains unblocked beforehand.
SIGINT/SIGTERM stop both CarParams and IPC waiting. TICI requests source cores
0–3 and FIFO5; host scheduling is unchanged. `DEBUG` is integer-valued;
`--demo` retains the source's no-op behavior. `--frames N` is a host-QA bound on
publications and still performs the same-frame cache write before exiting.

## Native numerical dependency

The Rust estimator calls the original `dgesdd` SVD method with reduced matrices
through a bounded FFI wrapper. Its supported artifact is NumPy 2.5.3's native
OpenBLAS 0.3.34.106.0 ILP64 library (`scipy_dgesdd_64_`). No Python or NumPy import
occurs in the daemon. `--numerics DIRECTORY`, `TORQUED_NUMERICS`, or the executable
sibling `torqued-numerics` selects the required artifact. Missing libraries,
unsupported manifest/version/ABI or mismatched file hashes are explicit errors.
The artifact is trusted executable code; its manifest hashes check packaging
consistency, not authenticity of an arbitrary user-supplied library.

Build-time staging is reproducible:

```sh
python -m pip download --only-binary=:all: --no-deps numpy==2.5.3 --dest /tmp/torque-wheels
python rust/tools/stage_torque_numerics.py --wheel /tmp/torque-wheels/numpy-2.5.3-*.whl --output /tmp/torque-numerics
# For GNU aarch64, download the appropriate wheel before staging:
python -m pip download --only-binary=:all: --no-deps --platform manylinux_2_27_aarch64 --python-version 312 --abi cp312 numpy==2.5.3 --dest /tmp/torque-arm-wheels
```

Staging retains archive/library SHA256 hashes, manifest, source provenance and
all wheel license notices. Verified Linux CPython312 wheel archives:

| Architecture | Source-locked NumPy 2.5.3 wheel SHA256 |
| --- | --- |
| x86_64 manylinux2.27/2.28 | `b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3` |
| aarch64 manylinux2.27/2.28 | `76c2c1e6bfa5c84adc6434dfbf013aa92096a7985221762c8f11fedfd20fff58` |

The staged GNU shared libraries require compatible OS libc/libm and loader.
The ARM Fortran runtime additionally needs `libgcc_s.so.1` and `libz.so.1`; the
host wheel bundles its Fortran/quadmath libraries. A musl build of the Rust
workspace does not make these GNU libraries musl-compatible. The full runtime
package must select the GNU executable, include the native artifact/licenses,
and verify its OS dependencies against the intended AGNOS image. Generic builds
and QEMU checks here are not AGNOS or device validation.

## Evidence and numerical contract

The contract was declared before implementation: all sampled indices, bucket
admissions/order/counts, restore/reset branches and discrete fields are exact;
pose/history Float64 values allow absolute plus relative `2e-12`; fit/filter
Float64 values allow absolute plus relative `2e-10`; Float32 wire values allow
one adjacent ULP with a `2e-10` absolute cancellation floor. Nonfinite classes
are exact. No gate threshold or tolerance was widened after failures.

The source checker executes the actual estimator/helper classes and captures
the raw fit before serialization. It exercises positive/negative/near-zero and
ill-conditioned slopes, sparse/valid/decimated/full/overflowing buckets,
NaN-SVD reset, randomized samples, complete packets, cache identity mismatches,
partial/corrupt/empty caches, negative/NaN decay, timestamp/lag history, invalid
poses, calibration transforms, override/engagement/speed and lateral-acceleration
gates. The original main-loop AST is separately executed against original
SubMaster updates, including stale/invalid/silent inputs and dual fits.

Current 2.5.3 source/native results are captured separately in
`.omo/evidence/torqued-numpy253/` with unchanged tolerances:

| Scenario | Binary observable | Artifact |
| --- | --- | --- |
| Locked archive staging | both GNU wheels exactly match `uv.lock`; wrong version/hash and old runtime manifest rejected | `locked-wheels.json`, `version-rejection.json`, `numerics-*/manifest.json` |
| Host original estimator | 3,613 steps; 298,537 packet fields; five matching source errors | `reference-host-final/report.json` |
| Host original loop | 2,250 iterations; 450 publications; 12 cache fits | `loop-host-final/report.json` |
| ARM original source + native ARM SVD under QEMU | same 3,613 steps and 298,537 fields; no tolerance change | `reference-arm-final/report.json` |
| Native wrapper under ASan | same 3,613 steps and 298,537 fields; external OpenBLAS internals uninstrumented | `reference-asan/report.json` |
| Real native IPC/Params lifecycle | source packets, strict idle signal exits, separately bounded loop stop and durable drain | `native/report.json` |

Both new wheel builds report OpenBLAS 0.3.34.106.0 USE64BITINT; the producer never
imports Python. Exact commands, hashes, loader checks and remaining CI gates are
recorded in the new evidence `INDEX.json`.

### Historical initial 2.4.6 evidence

The following evidence remains in `.omo/evidence/torqued/` and records the
initial 2.4.6 implementation only. Its numerical/native runs are superseded by
the 2.5.3 checks above; use the initial commit to reproduce the historical tools.
These results do not establish current model/pipeline or device acceptance:

| Scenario | Invocation | Binary observable | Artifact |
| --- | --- | --- | --- |
| Estimator/source | `check_torque_reference.py --binary .../torque_trace --numerics ... --output ...` | 3,613 steps; 298,537 packet fields; five source error cases; exact RNG samples; Float64 fits/filters within declared contract | `reference-release-final/report.json`, `trace.jsonl` |
| Continuous original loop | `check_torque_loop.py --binary .../torque_loop --numerics ... --output ...` | 2,250 iterations; 450 publications (276 valid,174 invalid); 12 cache fits | `loop-final/report.json`, `trace.jsonl` |
| Real host IPC/Params | `check_torque_daemon.py --binary .../openpilot-torqued --numerics ... --output ...` | source packets; 4Hz nominal/timeout cadence; blocked-lock publication; cache reload; sparse DEBUG; signals and bound exit0 | `native-verified/report.json`, `.capnp` captures and daemon logs |
| GNU ARM SVD and source | Same estimator checker under QEMU with ARM Python/NumPy and ARM Rust/OpenBLAS | all 3,613 steps and 298,537 fields pass without changed tolerances | `reference-arm-source-final/report.json` |
| Rust core memory | `cargo +nightly-2026-09-29 miri test -p openpilot-torqued --no-default-features --lib --test estimator --locked` | five regressions pass with strict provenance, symbolic alignment and preemption | `miri-host.log` |
| Native FFI wrapper under ASan | Original estimator checker against nightly ASan `torque_trace` | 3,613 steps / 298,537 fields pass; no sanitizer failure; external OpenBLAS internals uninstrumented | `reference-asan/report.json`, `reference-asan.log` |
| Workspace regressions | `cargo test --workspace --locked` | exit0 | `workspace-tests.log` |

In the historical2.4.6 run, an extra ARM-vs-x86 source comparison exceeded the Float64 intercept tolerance
for a deliberately ill-conditioned `1e8` slope. Comparing ARM Rust against the
actual ARM NumPy original source passes with the unchanged contract. This
records platform backend variation; it does not claim cross-architecture bit
identity or silently relax the oracle. Host ARM-target Miri initially lacked an
actual aarch64 C++ compiler in the local environment; host Miri passes and the
CI ARM Miri job installs that compiler. Native library internals are outside
Miri's Rust memory model. A native QA signal deadline initially assumed the whole
shutdown would finish in two seconds. A repeated strace capture showed the IPC
thread correctly joining the writer while that writer was in `fsync` /
`jbd2_log_wait_commit`. The ten-second whole-process wait is only a harness safety timeout for source-required
durable draining, not a shutdown guarantee. Separate idle CarParams/IPC signal
scenarios keep the strict two-second exit assertion. With the Params lock held,
the daemon must stop the loop and join its writer within 0.5s (observed 0.1022s),
produce no publication for ten additional poll inputs, remain alive until the
lock is released, and retain the decoded expected cache value after exit.
Production code, 100ms IPC checks, cadence thresholds and numerical tolerances
are unchanged. Raw syscall/wchan evidence remains in
`signal-repeat/2/blocked.json` and `signal-strace.498952/498957`.

## Inherited DEBUG defect and remaining gates

[Issue #46](https://github.com/bin9208/openpilot-rust/issues/46) records an
inherited failure: 4,080 cached points serialize to 98,040 bytes in both source
and Rust, exceeding the 256,000-byte service queue's three-message constraint
(`3 * ALIGN(98040 + 8) = 294144`). The original Python/msgq send exits with
SIGABRT (-6); the Rust native bridge rejects the same send with exit1. The
checker `check_torque_debug_capacity.py` captures both actual failures under
`debug-capacity/`. Normal DEBUG=0 publication and disk persistence pass. Queue
size, sampling and point retention are deliberately unchanged; #46 remains open
for a separate behavior decision.

Independent review, exact-head Actions, production startup/manager integration,
full existing logging/upload conversion, AGNOS dependency verification, TICI
scheduling and eventual user device comparison remain separate gates. This
increment establishes no measured CPU savings, complete runtime delivery or
vehicle acceptance. Public user documentation is not changed.
