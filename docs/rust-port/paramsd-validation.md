# Native paramsd and CarKalman (#143)

Tracking: [#143](https://github.com/bin9208/openpilot-rust/issues/143), [full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline `46570176`. Native component host validation; no device acceptance.

## Pre-implementation contract

Rust owns paramsd orchestration, the nine-state project-owned CarKalman equations/constants, consumed Pose/PoseCalibrator transforms, current CarParams decoding, Params migration/retrieval/persistence, GPS side-channel writes and liveParameters serialization. Unchanged external rednose/Eigen retains numerical updates and rewind through an owned CXX handle. Every model evaluation uses the six physical constants decoded from actual startup CarParams; no vehicle identity or fixture result is built into the runtime. Python/Cython and generated project C++ models are test/development-only.

Before numerical implementation, comparisons are fixed at absolute plus relative `1e-9` for Float64 states, complete covariance, residuals and helper/model outputs, and absolute plus relative `2e-6` for Float32 cereal values. Status/validity decisions, active/reset behavior, threshold branches, input order, integer timestamps, selected GPS service, Params keys/operations and persistence cadence must match exactly. NaN/infinity classification must agree. A branch difference is a failure even when its numeric delta is within tolerance; no runtime threshold is changed.

The oracle executes unchanged VehicleParamsLearner/paramsd functions with the original CarKalman and actual Cython/rednose wrapper. Verification includes dynamically different physical CarParams, cached and corrupt inputs, calibration and pose transforms, yaw/roll fallback, clipping/hysteresis, inactive/rewind transitions, nonfinite reset, Float32 validity boundaries, replay stiffness retention, debug covariance, source main-loop gating/cadence, and a native IPC/Params process with persistence/restart/shutdown. Host evidence remains intermediate; production selection, full startup/upload, exact-SHA CI, ARM/AGNOS and first device comparison remain separate gates.

## Preserved behavior

`openpilot-paramsd` waits for CarParams, migrates/retrieves the existing cache and continuously consumes livePose, liveCalibration, carState and the UbloxAvailable-selected GPS service. The SubMaster policy, stable log-time ordering, all-check gating, active-speed/steering boundaries, calibrated yaw, roll fallback, hysteresis and clipping are retained. GPS writes remain independent of all-check validity. It publishes liveParameters on livePose updates and queues the same serialized bytes for LiveParametersV2 at `frame % 1200 == 0`.

The six physical constants come from actual CarParams for every filter instance. A scoped thread-local native callback context binds each numerical operation, including rewind replay, to its owned vehicle constants. The registered external solver does not retain a vehicle-global configuration. Constructor/reset inputs and returned snapshots own their storage. Invalid dimensions, observation kinds and nonfinite observation times fail before entering the numerical solver.

Cache migration preserves the source outer-invalid/inner-valid message, Float32 conversion, malformed-JSON retention, invalid-field removal, existing-V2 preference and UTF-8/BOM/UTF-16/UTF-32 byte decoding. Retrieval checks car identity and steer-ratio bounds; it does not reinterpret the stored validity flag. Debug `std` becomes a covariance diagonal directly, without squaring. Non-replay startup resets stiffness to one. Invalid covariance dimensions fail at initialization without deleting an otherwise readable cache. Nonfinite state resets preserve the pre-reset standard deviations published by the source.

The memory LastGPSPosition key is removed at startup. Two FIFO workers preserve the independent nonblocking Params queues and drain on clean shutdown. Original Params write/remove return codes are ignored just as in the Python caller. Normal invocation is continuous; `--frames N` bounds publications for validation. `--memory-root` requires an isolated `rust-probe-` prefix. Scheduling requests FIFO5 then cores 0..3 on TICI, bypassing both on PC. The catalog advertises a candidate without changing production selection.

## Portable validation commands

Use Rust 1.94.0, Clang, the existing native msgq/cereal prerequisites and Python with `numpy pycapnp pyzmq cffi Cython sympy==1.14.0`. Python development headers are needed only for the source oracle. Eigen 3.4.0 is pinned in `uv.lock` to `commaai/dependencies` commit `40e5d76de1b33a86c5181b63db6782d8f06da1da`; its `eigen.INCLUDE_DIR` supplies the include path. The build does not execute a Python model generator. Check free space before each build/install: at least 25 GiB plus estimated growth; recover 35 GiB before resuming if below the floor.

```sh
export CARGO_INCREMENTAL=0
export PYTHONPATH=.
export CXX=clang++
export PARAMSD_EIGEN_INCLUDE="$(python -c 'import eigen; print(eigen.INCLUDE_DIR)')"
python rust/tools/build_paramsd_oracle.py --output /tmp/paramsd-oracle --eigen-include "$PARAMSD_EIGEN_INCLUDE"
cargo build --manifest-path rust/Cargo.toml -p openpilot-paramsd --bins --examples --locked -j2
python rust/tools/check_paramsd.py --oracle /tmp/paramsd-oracle --trace rust/target/debug/examples/paramsd_trace --evidence /tmp/paramsd-evidence/estimator
python rust/tools/check_paramsd_loop.py --oracle /tmp/paramsd-oracle --trace rust/target/debug/examples/paramsd_loop --evidence /tmp/paramsd-evidence/loop
python rust/tools/check_paramsd_daemon.py --oracle /tmp/paramsd-oracle --target rust/target --evidence /tmp/paramsd-evidence/daemon
python rust/tools/check_paramsd_startup.py --binary rust/target/debug/openpilot-paramsd --evidence /tmp/paramsd-evidence/startup
cargo test --manifest-path rust/Cargo.toml -p openpilot-paramsd -p openpilot-manager-catalog --all-targets --locked -j2
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-paramsd -p openpilot-manager-catalog --all-targets --locked -j2 -- -D warnings
python rust/tools/check_paramsd_native.py --evidence /tmp/paramsd-evidence/native
```

The sanitizer helper instruments the production CXX/rednose and scheduler boundary with Clang ASan/UBSan on Linux x86_64. It verifies instrumentation symbols and checks owned filter copies, snapshot lifetime, malformed boundaries, rewind eviction/reset and interleaved/concurrent vehicle models. It does not claim whole-process sanitizer coverage. Safe Rust tests run using `cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-paramsd --no-default-features --tests --locked -j2`, followed by strict provenance/symbolic alignment, preemption, and Tree Borrows configurations. Miri does not execute the C++/kernel boundary.

Development regeneration uses `python rust/tools/generate_car_model.py` and `python rust/tools/generate_car_corpus.py --oracle /tmp/paramsd-oracle`, then package Cargo fmt. The checked-in corpus captures all model/observation functions and Jacobians at 24 states with different physical constants; the equations retain source expression structure. Original source licenses and provenance remain in the repository.

## Evidence and limits

Evidence is retained under `.omo/evidence/paramsd-143/recovery/` in the parent workspace. The estimator gate compares seven cases, 1,196 operations and 148,302 floating-point values with zero observed difference. The cache/full-main gate compares 39 cases and 2,420 frames, including four saves and 216 GPS writes, with zero observed difference. Exception text after the diagnostic category is runtime-specific; log level/category, key operations, outputs and decisions are compared.

The executable gate exchanges 1,202 source-compared liveParameters messages through original msgq, checks the frame-1,200 serialized cache, then restarts from it for eight more source-compared messages. The initial native subscription-registration timeout consumes frame zero, so the first saved publication is index 1,199. Both GPS selections, actual Params files, drained writes and SIGINT/SIGTERM exit zero are observed. `/proc/PID/exe` and mapped libraries prove the native executable runs with an empty executable PATH and without Python, Cython or the generated source CarKalman library. Only the live publication clock is compared against exchange bounds instead of a fixed source clock.

Startup scenarios exercise interruptible missing-CarParams waiting, malformed CarParams, legacy migration, malformed JSON, missing fields, malformed V2 and invalid covariance dimensions. The source comparison caught and fixed the migration outer-valid default; a separate source inspection corrected byte-encoding compatibility. Host fixtures are synthetic and isolated: no device, vehicle, private route, NAS or production selection was used. They do not establish CPU savings, loaded timing, ARM/AGNOS execution or the complete normal-startup/log-upload delivery gate. Those and exact-SHA Actions remain parent integration work.
