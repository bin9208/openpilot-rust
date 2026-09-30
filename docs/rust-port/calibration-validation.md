# Rust calibration estimator and daemon

Issue [#35](https://github.com/bin9208/openpilot-rust/issues/35) is an intermediate
part of [#1](https://github.com/bin9208/openpilot-rust/issues/1), not a device
handoff. Source base: `277964e35f3a12037d143288d5641db3ed90e240` in this independent
repository. Algorithm provenance: `openpilot/selfdrive/locationd/calibrationd.py`,
`openpilot/common/transformations/{orientation,transformations}.py`, the complete
cereal schemas, original messaging classes and `openpilot/common/params.h`.
The original license and files remain unchanged.

## Implemented behavior

`openpilot-calibrationd` runs continuously, using the merged Rust
SubMaster/PubMaster and Params implementations. `--frames N` optionally bounds
**publications**, including the startup and timeout publications used for QA.
It uses the original service catalog and `OPENPILOT_PREFIX` namespace. It waits
for nonempty `CarParams` in Params before calibration starts; CarParams is not an
IPC subscription. `cameraOdometry` drives the poll, with `carState` as the other
subscription. The original `PARAMS_ROOT`, host HOME/.comma-prefix layout and
TICI `/data/params` selection are retained within the Rust Params namespace API.

The estimator preserves the 100-sample blocks and 50-block ring, five-block
minimum, the source `get_valid_idxs` exclusion, linear-decay weighting, strict
speed/yaw/uncertainty gates, spread reset, recalibrating status and smoothing.
RPY composition follows the source Z-Y-X matrices and matrix-to-quaternion-to-
Euler conversion. Standard and mici pitch limits remain distinct and unchanged.
Source nonfinite clipping and saved height/wide-array fallbacks are retained.
Finite malformed saved RPY arrays keep the source's observable success/error
boundary; invalid dimensions or out-of-range history counts produce typed errors
where the original raises instead of silently accepting different calibration.

The complete `liveCalibration` packet preserves the not-car override, defaults,
progress, spread, wide alignment, height and Float32 field rounding. Cached
`CalibrationParams` are decoded from cereal before promotion to Float64 state;
the saved status is recomputed as in the original. Unreadable cached messages
log an error and use the source defaults/partially assigned fields. Finite
malformed arrays that fail the original estimator remain fatal.

`CameraYawTrimDeg` is read only for an updated camera message. The parser uses
`strtof`, matching the original C++ `std::stof`, then promotes the Float32 result
to Float64 before multiplying by 0.01. Prefix parsing, embedded NUL, hexadecimal
forms, NaN/infinity and range errors are tested against the actual extracted
`Params::getFloat` method. Trimming freezes calibration only after the internal
status is calibrated; neither the strict `1e-6` comparison nor validity gates
are changed.

The first SubMaster update has timeout zero; subsequent updates have 100 ms.
Publication occurs at `frame % 5 == 0`, with `sm.all_checks()` validity. This is
4 Hz with 20 Hz camera input, and still publishes every fifth timeout update
when input is absent. Invalid incoming Events do not suppress estimator input,
matching the source loop, but they affect outgoing validity through SubMaster.

At the source persistence points (completed blocks 5, 15, 25, 35 and 45), the
serialized message enters a FIFO worker queue. Durable Params writes and their
lock/fsync run off the IPC thread; write failures are logged. Orderly shutdown
drains this queue, like the original Params destructor. SIGINT/SIGTERM interrupt
CarParams and IPC waiting with bounded checks. Shutdown may wait for an already
blocked durable write to finish; it does not claim cancellation of filesystem
I/O. On TICI, source cores 0–3 and FIFO priority 5 are requested; hardware model
selection reads the original device-tree model path. This branch is not run in
host QA and remains device acceptance work.

## Local evidence (2026-09-30)

Evidence root in the issue worktree: `.omo/evidence/calibrationd/`. `INDEX.json`
records exact commands, binary hash, revision and paths. No vehicle data is used.

| Scenario | Observed result | Artifact |
| --- | --- | --- |
| TDD before implementation | Missing estimator imports fail compilation | `red.log` |
| Original calibration class | 15,128 steps, 256,657 packet fields, 30 matching error cases and 14 persistence triggers | `oracle-verified/report.json`, `oracle-verified/trace.jsonl` |
| Original main-loop body and SubMaster | 3,600 updates, 720 publications, 525 frozen updates; valid/invalid and timeout cases agree | `loop-oracle-verified/report.json`, `loop-oracle-verified/trace.jsonl` |
| Original C++ `getFloat` | Exact Float32 promotion/prefix/range behavior, including adjacent trim-boundary values | `float-oracle-verified/report.json`, generated `source_get_float.cc` |
| Native original Python IPC | Complete source packets, three original service capacities, nominal 250 ms and timeout 500 ms cadence | `native-verified/report.json`, `native-verified/cadence/message-*.capnp` |
| Real Params lock contention | 520 input frames; publications continue while durable write is blocked, then the source-matched saved packet appears | `native-verified/persistence/saved.capnp`, `native-verified/report.json` |
| Native saved reload and yaw trim | 300 frames covering Float32 boundary, freeze and resume | `native-verified/freeze-reload/message-*.capnp`, `native-verified/report.json` |
| Native not-car/cache/errors/signals | Meaningful nonzero-cache not-car override; corrupt-cache fallback; malformed finite-array failure; both signals in CarParams/IPC waits; bounded exits | `native-verified/report.json` |
| Focused Rust regressions and lint | Seven tests, formatting, Clippy and Python lint pass | `tests.log`, `fmt.log`, `clippy.log`, `ruff.log` |
| Pure estimator/wire memory handling | Five tests pass with strict provenance, symbolic alignment and preemption | `miri.log` |
| Native parser/persistence memory handling | Two tests pass under ASan | `asan.log` |

Tolerances were set before the comparisons: Float64 estimator/orientation state
uses absolute and relative `2e-12`; Float32 packet values allow one adjacent
representable value or the same `2e-12` cancellation floor. Discrete values,
array lengths, accepted/rejected decisions, status, persistence cadence and
NaN/infinity classes must agree. Thresholds and tolerances were not widened
after failures. An initial native fixture used two decimal strings that rounded
to the same Float32 value; the C++ oracle identified this, and the fixture now
uses the actual next Float32 value without changing runtime code.

## Reproduction and CI

Run from the repository root, with NumPy and pycapnp installed. The native check
also needs the original msgq Python binding built by `rust/tools/build_msgq_python.py`.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-calibrationd --bins --examples --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-calibrationd --all-targets --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-calibrationd --all-targets --locked -- -D warnings
PYTHONPATH="$PWD:$PWD/rust/tools" python rust/tools/check_calibration_reference.py \
  --binary rust/target/debug/examples/calibration_trace --output /tmp/calibration-estimator
PYTHONPATH="$PWD:$PWD/rust/tools" python rust/tools/check_calibration_loop.py \
  --binary rust/target/debug/examples/calibration_loop --output /tmp/calibration-loop
python rust/tools/check_calibration_float.py \
  --binary rust/target/debug/examples/calibration_float --output /tmp/calibration-float
PYTHONPATH="/path/to/msgq-python:$PWD:$PWD/rust/tools" python rust/tools/check_calibration_daemon.py \
  --binary rust/target/debug/openpilot-calibrationd --output /tmp/calibration-native
```

Rust CI repeats these source/native checks, retains reports and native packets,
and includes the executable in host and generic GNU/musl aarch64 workspace
builds. The memory job includes ARM Miri and the native Params ASan tests. Local
ARM Miri attempts stopped at missing cross C++ compiler/headers; the CI memory
job now installs that prerequisite. Those failed prerequisite attempts are
retained in `miri-arm.log` and `miri-arm-clang.log`; they are not ARM test success.
Exact-head cloud, independent review and post-merge results remain integration
gates owned by the parent task.

## Remaining acceptance

Production process selection is unchanged. TICI/mici hardware execution,
AGNOS scheduling, whole-runtime manager/startup/logging/existing upload flow and
the user's eventual original-versus-Rust device comparison remain pending.
Native msgq, libc and OS interfaces remain explicit external dependencies.
No device was accessed or requested, and no CPU, thermal or vehicle-performance
improvement is claimed. The first device handoff remains gated on the complete
project-owned runtime described in [design.md](design.md).

Docs-Not-Needed: internal daemon port and host verification only; no settings or
production manager behavior changed.
