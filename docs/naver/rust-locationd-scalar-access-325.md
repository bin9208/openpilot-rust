# locationd scalar filter reads (#325)

Tracking: [#325](https://github.com/bin9208/openpilot-rust/issues/325),
[full runtime retirement #296](https://github.com/bin9208/openpilot-rust/issues/296).
Source context: parent branch `codex/feat-296-runtime-retirement`, parent HEAD
`7c8b2f0f05b1140d6d391275d3643ba3daa769b1` during validation; changes remained
uncommitted for parent integration.

## Change and preserved boundary

Timestamp validation previously created a complete Kalman snapshot to consume
only its time. Primary gyro sanity validation created another complete snapshot
to consume only state index 11. Each snapshot copied 18 state and 324 covariance
doubles into two Rust Vecs, in addition to the Eigen copies inside the external
state/covariance APIs. Neither caller used its covariance.

Two scalar CXX accessors now return the current filter time and gyro bias z.
`Filter::filter_time` calls the external `get_filter_time` directly;
`Filter::gyro_bias_z` reads `filter->state()(11)`. Safe PoseKalman wrappers retain
the existing null-handle contract and return owned f64 values. Only the two
scalar-use estimator expressions changed. Input order, arithmetic grouping,
thresholds, source policy, logs, publication snapshots, numerical model, updates,
reset, rewind and finite checking remain unchanged.

The external rednose implementation is unchanged. Its state() returns an owned
Eigen vector, so the gyro bias accessor still copies 18 doubles internally.
This is not a zero-allocation implementation of accepted gyro processing or an
allocation-free C++ path. No Rust slice or reference outlives the synchronous
FFI call. Scalar reads intentionally fetch current state rather than cache a
returned Estimate: delayed observation results precede replayed future history.

## RED/GREEN and native equivalence

A thread-local allocation counter surrounds 100 primary gyro validation calls
that are rejected by the existing norm limit before observe. It verifies every
InputInvalid result, no diagnostics, and unchanged complete filter state and
covariance outside the measured region. Before the production change, this
fixture failed with 400 Rust allocations. Afterward it passed with zero Rust
allocations in that measured path. C++ Eigen allocation is outside the counter's
visibility. No execution-time or whole-device CPU saving was measured.

Two added native ownership cases compare scalar values bit-for-bit against owned
snapshots after construction, independent handles, reset, signed zero,
nonfinite seeds, ordinary observations, delayed rewind/fast-forward, rejected
old observations and history-clearing reset. The delayed fixture asserts its
returned Estimate bias differs from current replayed state; this detects a stale
Estimate cache instead of merely repeating the new accessor implementation.
Existing owned-snapshot lifetime/dimension/history regressions remain present.

A pre-change native trace was built and retained only in ignored owned scratch
(SHA256 `89186379b0dfe41263e2daa6efa3239e2296be759717c59066b4af12eb016900`).
The pre/post executables produced byte-identical complete JSON output across all
8 existing cases and 1,073 steps: 7,842,092 bytes including states, covariance,
encoded publications, decisions and logs.

The unchanged original Python LocationEstimator/PoseKalman and actual generated
C++ model/Cython rednose oracle were rebuilt independently in scratch. The
existing source comparison passed 8 cases, 1,073 steps and 457,096 numerical
comparisons; maximum absolute difference was `1.7763568394002505e-15`.
Float64 `1e-9`, Float32 cereal `2e-6`, invalid counters `1e-12`, exact decisions/
flags/integer timestamps and NaN classification policies were not changed.
Original Python/Cython remain explicit interim external comparison providers;
the new runtime getters do not invoke them or weaken the generated oracle.

## Commands and results

Canonical environment: `/home/fma1/.config/openpilot-rust/env.sh`, with
`OPENPILOT_RUST_ROOT` and `PYTHONPATH` pointing at the current worktree;
shared `/home/fma1/rust/cache/cargo-target`, incremental compilation disabled,
two jobs. Required disk checks before each build/copy retained over 372 GiB free
against the 25 GiB reserve plus 1–2 GiB estimated build growth.

- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-locationd`:
  all 12 integration tests passed (model 1, ownership 6, scalar allocation 1,
  wire 4).
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-locationd --all-targets -- -D warnings` passed.
- `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-locationd --example location_trace` passed before and after the change.
- `python rust/tools/build_locationd_oracle.py --output <scratch>/oracle --eigen-include "$LOCATIOND_EIGEN_INCLUDE"` built the unchanged source provider.
- `python rust/tools/check_locationd.py --oracle <scratch>/oracle --trace /home/fma1/rust/cache/cargo-target/debug/examples/location_trace --evidence <scratch>/estimator` passed.
- `python rust/tools/check_locationd_native.py --evidence <scratch>/native-sanitizer` passed ASan/UBSan instrumented production filter/callback ownership (six tests) and scheduler fixtures. The receipt now derives its count from the single successful nonempty Rust test summary; the actual six-test log plus six negative controls (missing, empty, failed, nonzero failures, duplicate and incomplete summaries) validated the parser without another Cargo build.
- Focused Rustfmt and `git diff --check` passed.

The first allocation-test build required an explicit test-only unsafe allowance
for the System-delegating allocator. The first original-oracle compile supplied
the Eigen directory one level too deep for the source's `eigen3/Eigen` include;
using the canonical existing Eigen include root corrected that environment.
Neither failure required changing production math or validation criteria.

Evidence root: `.analysis/scratch/2026-10-14-locationd-scalar-audit/` (ignored).
No dependency installation, device access, commit, push or target acceptance was
performed by this worker. Parent must rebuild the source-matched AGNOS locationd
binary and retain normal startup/recording/upload and driving acceptance as
separate gates. No complete locationd/module clearance follows from this change.

Docs-Not-Needed: internal optimization evidence without settings or user-guide behavior changes.
