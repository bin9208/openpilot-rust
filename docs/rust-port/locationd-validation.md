# Native locationd and PoseKalman (#138)

Tracking: [#138](https://github.com/bin9208/openpilot-rust/issues/138), [full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline: `759e4e0a`. Native component host validation; no device acceptance.

## Boundary and pre-implementation numeric contract

Rust owns locationd input ordering, sensor/time/sanity checks, initialization and Params decoding, invalid-input decay, calibration rotation, publication, scheduling, and PoseKalman model equations/constants. The unchanged external rednose EKFSym implementation owns numerical updates, covariance propagation and its 512-checkpoint rewind mechanism through an owned CXX handle. Eigen remains an external native numerical dependency. Generated Rust equations are derived from the unchanged project-owned symbolic PoseKalman definitions during development; the runtime does not load Python, Cython or a generated model shared library.

Before numerical implementation, comparisons are fixed at absolute plus relative `1e-9` for Float64 states, complete covariances, residuals, and model/Jacobian outputs; invalid-input counters use `1e-12`. Float32 cereal measurements use absolute plus relative `2e-6`, with both exact emitted values and error maxima retained. Status enums, validity flags, sensor source policy, observations accepted/rejected, integer timestamps, ordering and initialization/reset decisions must match exactly. NaN/infinity classification must agree; nonfinite filter state/covariance must trigger the source reset, not become a successful numerical comparison. These tolerances address floating-point expression/compiler rounding only and do not change a runtime threshold.

The source oracle executes the unchanged Python LocationEstimator/PoseKalman with the actual Cython rednose wrapper and generated source C++ model. Fixtures cover normal ordered inputs, delayed camera rewind, rewind boundaries/exhaustion, invalid/stale/source-rejected sensor data, nonfinite reset, calibration and odometry checks, initialization Params, invalid-count recovery, posenet spike behavior and native IPC/termination. Host evidence is intermediate; complete startup/log upload, exact-SHA Actions, ARM/AGNOS ABI and first device comparison remain separate gates.

## Preserved runtime behavior

`openpilot-locationd` subscribes to carState, liveCalibration and cameraOdometry using the original SubMaster policy, with separate non-conflated accelerometer/gyroscope drains. It preserves the stable log-time ordering, camera EOF delay, first-cycle initialization behavior, frequency/validity checks, sensor liveness, invalid-input counters and recovery, source diagnostics, calibration transforms and livePose debug fields. Sensor vectors consume the first three elements; only BMX055 is source-rejected, including when a future source enum is unknown. Source-invalid events are not interpreted.

LocationFilterInitialState remains a read-only startup seed. Its stored `std` values become the covariance diagonal directly, preserving the source behavior rather than squaring them. Empty lists use source defaults. Invalid dimensions fail before the native boundary. Nonfinite updates reset the filter as in the source, while preserving the source observation bookkeeping and initialized flag. Uninitialized filter time becomes zero; a negative or unrepresentable UInt64 publication timestamp fails rather than silently saturating.

The native scheduling boundary requests FIFO priority 5 followed by cores 0..3 on TICI, and bypasses both operations on PC. No scheduling syscall is applied to a device during verification. Normal invocation runs continuously; `--frames N` bounds publications for host fixtures. DEBUG and SIMULATION retain integer flag semantics. The catalog exposes a candidate; production process descriptors and selection are unchanged. paramsd, lagd and unrelated GNSS work are outside this component.

## Reproduction

Use Rust 1.94.0, Clang, existing native msgq/cereal prerequisites, and Python with `numpy pycapnp pyzmq cffi Cython sympy==1.14.0`. Python development headers are required for the source Cython oracle. The repository's Eigen dependency is 3.4.0 at `commaai/dependencies` commit `40e5d76de1b33a86c5181b63db6782d8f06da1da`; set `LOCATIOND_EIGEN_INCLUDE` to that package's `eigen.INCLUDE_DIR`. Cargo does not execute the Python model generator.

```sh
export CARGO_INCREMENTAL=0
export PYTHONPATH=.
export CXX=clang++
export LOCATIOND_EIGEN_INCLUDE="$(python -c 'import eigen; print(eigen.INCLUDE_DIR)')"
python rust/tools/build_locationd_oracle.py --output /tmp/locationd-oracle --eigen-include "$LOCATIOND_EIGEN_INCLUDE"
cargo build --manifest-path rust/Cargo.toml -p openpilot-locationd --bins --examples --locked -j2
python rust/tools/check_locationd.py --oracle /tmp/locationd-oracle --trace rust/target/debug/examples/location_trace --evidence /tmp/locationd-evidence/estimator
python rust/tools/check_locationd_loop.py --oracle /tmp/locationd-oracle --trace rust/target/debug/examples/location_loop --evidence /tmp/locationd-evidence/loop
python rust/tools/check_locationd_timestamp.py --oracle /tmp/locationd-oracle --trace rust/target/debug/examples/location_trace --evidence /tmp/locationd-evidence/timestamp
python rust/tools/check_locationd_daemon.py --oracle /tmp/locationd-oracle --target rust/target --evidence /tmp/locationd-evidence/daemon
cargo test --manifest-path rust/Cargo.toml -p openpilot-locationd --tests --locked -j2
python rust/tools/check_locationd_native.py --evidence /tmp/locationd-evidence/native
```

The sanitizer helper targets Linux x86_64 Clang runtime libraries and instruments the production CXX/solver code and scheduler. It does not instrument the whole runtime. Safe Rust model/serialization tests run with `cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-locationd --no-default-features --test model --test wire --locked -j2`, then again with `MIRIFLAGS='-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check -Zmiri-preemption-rate=0.1'`. Miri cannot execute C++ or kernel operations; those boundaries use the sanitizer scenarios instead.

Development regeneration: `python rust/tools/generate_pose_model.py`, `python rust/tools/generate_pose_corpus.py --oracle /tmp/locationd-oracle`, then focused Cargo fmt. The generated Jacobian file retains the source expression structure, including repeated subexpressions; it is a generated numerical artifact rather than hand-maintained orchestration.

## Host evidence and limitations

Evidence is under `.omo/evidence/locationd-138/` in the parent workspace. The estimator gate matches eight cases/1,073 steps and 457,096 floating-point values with maximum absolute error `1.7763568394002505e-15`. The unchanged source main loop matches four cases/676 frames with maximum absolute error `3.637978807091713e-12`, including initialization, invalid/recovery transitions, real-clock and simulation sensor policies, message validity, ordering and seeded covariance. All decisions/flags and integer timestamps match exactly.

The continuous native process gate compares 115 livePose publications plus eight seeded-startup publications against the actual source main loop. It uses owned msgq namespaces and temporary Params, SIMULATION=1, an empty executable PATH and `/proc/PID/exe`/maps inspection. Python, Cython and the generated source C++ model library are absent from the daemon's mapped libraries. SIGINT and SIGTERM exit zero in about 114 ms while idle. Publication clock values are checked against the live exchange bounds; only those wall-clock-dependent values are removed from the source comparison. Deterministic gates compare them too. This is not a loaded timing/performance measurement.

Two edge regressions were reproduced before repair: extra sensor components/unknown source IDs were initially rejected, and negative filter timestamps were initially saturated. The source-compatible fixes pass the complete affected comparisons. The first IPC fixture also exposed msgq publisher initialization invalidating readers; the harness now observes actual reader registration before sending, without changing daemon or transport policy.

Ownership tests cover constructor/reset input copies, independent handles, snapshots surviving destruction, rejected dimensions/kinds, 512-checkpoint eviction, rewind replay and reset cleanup. Scheduler fixtures cover PC bypass, FIFO5/core mask order and syscall failures without invoking real scheduling changes. Native dependencies remain rednose, Eigen, CXX, original msgq/cereal, Params/logging and Linux APIs. No device, private route, vehicle, NAS, sensor hardware or production selection was used. Exact-SHA CI, generic ARM builds, AGNOS ABI, loaded behavior and the complete normal-startup/upload delivery gate remain parent integration work.
