# Native lateral lag estimator (#139)

Issue [#139](https://github.com/bin9208/openpilot-rust/issues/139), whole-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline `82c3dba8`.
This is a host-validated continuous runtime candidate, not device acceptance.

## Numerical contract fixed before implementation

Commit `31cd12ed` fixed `rust/crates/lagd/tests/tolerances.json` before code was
written. Absolute + relative budgets are 1e-12 for pose/smoothing, 1e-9 for
normalized correlation, 1e-8 for lag/block statistics, and 1e-7 for serialized
Float32 fields. Finite/nonfinite classification and discrete candidate runs,
indices, widths, acceptance, masks, validity/status, cadence, counters and Params
actions must agree. Tolerances never excuse a different policy decision, and no
production threshold was changed.

The initial RustFFT 6.4.1 candidate failed that contract for constant input: the
source selected 0.15 seconds while RustFFT selected 0.5 seconds. Cancellation in
the normalized correlation makes the numerical backend observable in this case.
The failed evidence is retained; tolerances were not widened.

The retained external kernel is the exact PocketFFT revision pinned by reference
NumPy 2.5.3. `openpilot-pocketfft` builds the vendored BSD-3-Clause header through
a small CXX ownership adapter, with no NumPy/Python interpreter or shared-library
runtime dependency. The Rust-owned implementation still performs smoothing,
masking, normalized correlation arithmetic, lag/candidate selection, block/window
state, calibration, validity, serialization and daemon policy. See
[the kernel provenance/boundary notice](../../rust/crates/pocketfft/NOTICE.md) and
[the revision/hash manifest](../../rust/crates/pocketfft/provenance.json).

Masked constant inputs also exposed complex-multiplication rounding: native FFT
arrays matched NumPy exactly, while 84 of 220 product bins differed with separate
multiply/add operations. NumPy's pinned `simd_cmul_f64` uses fused multiply/add
for both x86 AVX2/FMA3 and aarch64 NEON. Rust now spells that arithmetic with
`f64::mul_add`; the trace and all 24 added masked-constant cases pass without
changing a candidate decision or tolerance. This matches the pinned reference's
FMA dispatch, not every possible NumPy build/CPU dispatch. Actual aarch64
execution remains a separate gate; host parity does not establish target parity.

## Implemented scope and preserved behavior

`rust/crates/lagd` ports `openpilot/selfdrive/locationd/lagd.py` and its required
helpers from `locationd/helpers.py`. Existing Rust calibration rotation and
scheduler/clock helpers are reused. The pose helper includes vector/covariance
rotation and the calibrated orientation, velocity, acceleration and angular
velocity fields; no locationd/PoseKalman implementation is changed.

- Gaussian masked symmetric smoothing, source FFT padding, masked normalized
  cross-correlation, parabolic peak interpolation and candidate confidence.
- The full 60-second point window, 25-second valid-data minimum, recovery buffer,
  velocity/yaw/lateral-acceleration gates and new-data checks.
- The 50 by 100 block history, partial/current block exclusion rules, progress,
  standard-deviation status threshold, initial-lag fallback and delay clipping.
- Timestamp-sorted input handling with source insertion order preserved on ties.
  `livePose` validity uses exactly the source's angular-valid, posenet and input
  flags; unrelated orientation-valid/sensorsOK fields do not add new gates.
- Source-compatible filesystem-read-error handling as empty Params values, native
  `CarParams` wait, cached `LiveDelay`/`CarParamsPrevRoute` restore and
  removal policy, debug points, source scheduling, continuous msgq publication,
  asynchronous durable Params writes, and owned writer shutdown.
- Source publication cadence (`frame % 5`) and persistence (`frame % 1200`),
  including frame 0. Poll timeout remains the source's 1000 ms.

Two easily missed source rules remain unchanged. Candidate-run lookup uses its
existing index without adding the minimum-lag sample offset. Estimation and
publication still run on their cadence when subscription checks fail; only new
point ingestion is suppressed, and the outer `liveDelay.valid` reflects those
checks. Cached restoration checks the source fingerprint/count/status fields,
not a newly invented outer-validity requirement. Negative cached block counts
and unknown status ordinals follow the observed source behavior; progress that
cannot fit the schema remains an error.

## Focused evidence

Artifacts are under `.omo/evidence/lagd-139/` in the issue worktree. The final
plain-text receipt contains actual command output; the ledger names invocations,
observables, artifact paths and dependency hashes.

| Scenario | Observable | Artifact |
| --- | --- | --- |
| Numerical source comparison | 95 cases pass fixed tolerances; exact candidate run/index/width; 24 masked-constant cases included | `fma-numeric.json`, `fma-numeric.log` |
| Rejected numerical backend | RustFFT constant-input candidate differs; retained as a failure, not acceptance | `numeric.json`, `numeric.log` |
| Arithmetic localization | Exact FFT equality, 84 plain-product differences, exact fused-product equality | `arithmetic-trace.json`, `arithmetic-trace.log`, `masked-constant.json` |
| Unchanged full `lagd.main` body | 3,703 frames agree, including internal masks/recovery/block counters and last-estimate timestamps; writes at 0/1200 | `fma-loop/{normal,recovery,ordering}/{source,native,result}.json`, `fma-loop.log` |
| Cached Params policy | 14 restore/reject/remove cases: missing/empty/corrupt, fingerprint, count, status, nonfinite values | `cache.json`, `cache.log` |
| `liveDelay` boundaries | 13 cases: estimated/unestimated/invalid, exact STD boundary, clipping, partial/full ring, negative progress | `packet.json`, `packet.log` |
| Real native learning | 1,201 synthetic input frames, 241 publications, learned blocks, actual private Params persistence; final invalid publication retained | `fma-daemon/learning/` |
| Native cache/cadence/shutdown | Corrupt-cache recovery; real 20 Hz input/4 Hz output; SIGTERM during CarParams wait | `fma-daemon/{corrupt,cadence}/`, `fma-daemon/wait-signal.json`, `fma-daemon.log` |
| Params filesystem errors | CarParams directory remains a wait; LiveDelay directory is treated as absent; source remove return-code behavior retained | `params-io-red.json`, `params-io.json`, `params-io.log` |
| Foreign-kernel boundary | Owned plan/checked slice tests plus 224 forward/inverse round trips under ASan/UBSan | `fma-test.log`, `sanitizer.log` |
| Static/package checks | Focused package tests, warning-denying Clippy, Rust/Python format and Python lint/syntax | `fma-test.log`, `clippy.log`, `fmt.log`, `python-lint.log`, `static-checks.log` |

The long native learning stream uses the source-supported `SIMULATION=1` in an
isolated namespace to accelerate synthetic input without changing daemon policy.
A separate `SIMULATION=0` case checks real host cadence. These tests are not a
performance measurement or a drive replay. No real device, private route, NAS,
C3X, production Params namespace or production topic is accessed.

The CXX kernel cannot execute under Miri. It is isolated behind the existing
`native-skip-miri` convention; owned buffers, checked slice extents, CXX exception
conversion, focused native tests and ASan/UBSan are the alternate boundary
validation. No Miri execution of foreign code is claimed.

## Portable focused CI commands

Use the repository's existing Cap'n Proto/C++/msgq prerequisites and Python
NumPy **2.5.3** plus pycapnp. No PocketFFT download is needed: the pinned header
and license are committed. Check disk before each build, retain the 25 GiB reserve
plus growth, and use a coordinated cache with incremental compilation disabled.

```sh
export CARGO_INCREMENTAL=0
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-lagd -p openpilot-pocketfft -- --check
cargo test --manifest-path rust/Cargo.toml -p openpilot-lagd -p openpilot-pocketfft --locked -j2
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-lagd -p openpilot-pocketfft --all-targets --locked -j2 -- -D warnings
cargo build --manifest-path rust/Cargo.toml -p openpilot-lagd --bins --examples --locked -j2
export PYTHONPATH=.
target_dir="${CARGO_TARGET_DIR:-rust/target}"
python rust/tools/check_lagd_numeric.py "$target_dir/debug/examples/lag_numeric" evidence/lagd/numeric.json
python rust/tools/check_lagd_loop.py "$target_dir/debug/examples/lag_loop" evidence/lagd/loop
python rust/tools/check_lagd_cache.py "$target_dir/debug/examples/lag_cache" evidence/lagd/cache.json
python rust/tools/check_lagd_packet.py "$target_dir/debug/examples/lag_packet" evidence/lagd/packet.json
python rust/tools/check_lagd_daemon.py "$target_dir/debug/openpilot-lagd" "$target_dir/debug/examples/lag_ipc_peer" evidence/lagd/daemon
python rust/tools/check_lagd_params_io.py "$target_dir/debug/openpilot-lagd" evidence/lagd/params-io.json
clang++ -std=c++17 -O1 -g -ffp-contract=off -fsanitize=address,undefined -fno-omit-frame-pointer rust/crates/pocketfft/native/kernel_test.cc -o evidence/lagd/pocketfft-sanitized
ASAN_OPTIONS=detect_leaks=1 UBSAN_OPTIONS=halt_on_error=1 evidence/lagd/pocketfft-sanitized
```

For the existing generic ARM gate, build `openpilot-lagd` and its static
`openpilot-pocketfft` dependency with the configured aarch64 GNU C/C++ compiler
and linker, for example the existing workflow's
`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc` setup and
`cargo build --manifest-path rust/Cargo.toml -p openpilot-lagd --release --target aarch64-unknown-linux-gnu --locked -j2`.
Host source comparison, a generic cross-build and AGNOS execution are separate
claims. This worker does not claim the ARM/device gate.

## Remaining integration

The catalog records candidate availability only; production process selection
remains unchanged. Native dependencies are the external PocketFFT kernel, the
existing msgq C++ adapter, and Linux scheduling/clocks/filesystem plus native
Params/logging. The torque crate is reused only for scheduler/clock helpers;
lagd does not load its OpenBLAS artifact.

Exact-SHA Actions/ARM integration remains with the leader. locationd/PoseKalman
#138, paramsd, CI workflow edits and user guides are outside this change. Full
runtime #1 stays open until complete project-owned conversion, normal startup and
the existing log upload path are ready; the user performs the first device
comparison afterwards.

Docs-Not-Needed: isolated experimental runtime candidate; no production selection
or user-visible setting/public-guide behavior change.
