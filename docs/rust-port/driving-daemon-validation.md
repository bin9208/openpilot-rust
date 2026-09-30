# Internal driving-model daemon validation

Issue [#31](https://github.com/bin9208/openpilot-rust/issues/31), within full-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1) and
[#6](https://github.com/bin9208/openpilot-rust/issues/6).

## Implemented boundary

`openpilot-driving-modeld --trusted-catalog PATH` runs continuously using the
original `camerad` VisionIPC and cereal/msgq namespaces. `--frames N` bounds the
number of processed camera pairs, including prepare-only pairs. The trusted
catalog is the immutable CPU/QCOM artifact format already validated by the model
runtime. The source kernels and weights remain unchanged. Production manager
selection is unchanged.

The process selects road plus optional wide streams using the original typed
`UseWideCamera` value and defaults. Wide-only availability uses the original
single-stream fallback. Road subscriptions conflate; the extra wide subscription
does not. Pairing retains the source 20 ms SOF bound and ten-attempt limit. Camera
buffers are copied into owned NV12 staging before native execution; zero-copy
imports remain pending. Kernel execution preserves prepare-only image history,
desire pulses, traffic convention, action times and recurrent feature feedback.

The nine original message inputs use the shared service catalog's actual queue
capacities. Input validity does not newly gate source calibration or model
publication. Calibration updates require the original updated/seen combination,
and yaw trim applies only after calibration. The prior publication's desire is
the next model input. New model geometry is first narrowed through cereal before
the desire helper consumes it. Previous action feedback is likewise Float32,
matching the original Action builder.

The source parameter refresh counts loop iterations, including camera timeouts;
the desire helper retains its separate publication count. Numeric Params reads
use the source C++ `stof`/`stoi` conversion behavior, including Float32 promotion,
accepted numeric prefixes and range errors. Initial longitudinal delay includes
the source 0.3 seconds; the hundredth iteration uses the stored override exactly
as the original loop does. On TICI, the process requests core 7 and FIFO 54.

The three publications are `modelV2`, `drivingModelData` and `cameraOdometry`.
Calibration-seen validity, frame drops, actions, lane/turn state, side geometry,
retired turn-speed field, and raw prediction behavior follow the source. Any
nonempty `SEND_RAW_PRED`, including `0`, enables raw data. In simulation, pose EOF
is sampled after desire/path construction, immediately before pose serialization.
Normal pose EOF retains the camera capture timestamp.

SIGINT/SIGTERM stop discovery, CarParams waiting and camera receives. Discovery
and receive waits are bounded at 100 ms. Native inference completes synchronously
before the loop observes a shutdown request. A disconnected camera remains
unproductive until process restart, matching the existing daemon boundary.

## Native and source evidence

`rust/tools/check_driving_daemon.py` runs the actual compiled original driving
pipeline and selected original main-loop statements/functions. Its companion
`driving_daemon_reference.py` keeps source AST execution separate from transport.
It does not replace action, calibration, drop, desire or publication math with a
handwritten expected implementation. The C++ peer uses the original native
VisionIPC/msgq sources and generates its service table from original Python.

The final host run covers dual 1344x760, dual 1928x1208, road-only 1344x760 and
wide-only 1344x760. It compares 90 publications and 57,262 decoded fields, with
22 nonempty raw model outputs byte-identical. Discrete fields are exact. Existing
model-output numerical bounds remain 1e-6 for parsed fields and 2e-5 for path
coefficients. No tolerance was raised for this daemon.

Scenarios include initial prepare-only history, unseen calibration invalidity,
invalid-flag calibration updates and retention, RHD changes, desire transitions,
dynamic action-delay feedback, a later dropped frame after filter warmup,
continued recurrent state, raw on/off, all source queue capacities, bounded exit,
114 no-publication timeouts and nine signal checks. Independent packet re-decoding
compares the 180 retained expected/actual packet artifacts again. These short
native runs use refresh-invariant settings; the loop's hundredth-iteration
refresh and Float32 feedback have focused Rust regressions, while the longer
desire/input histories remain covered by #20/#24 source comparisons.

The host peer initially used raw msgq's default queue size. Original Python uses
service-specific capacities; generation from `services.py` repaired the oracle.
A second harness failure came from reading the wide image file after sending the
main image: that read exceeded the original 100 ms receive timeout. Preloading
both files before either send repaired the transport stimulus. Runtime pairing
and receive limits were not loosened.

Run with the original-model environment and a catalog compiled by the same LLVM
version as its source artifacts:

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-driving-modeld --locked
PYTHONPATH=.:tinygrad_repo:rust/tools DEV=CPU:LLVM JIT=1 JIT_BATCH_SIZE=0 \
  python rust/tools/check_driving_daemon.py --binary rust/target/debug/openpilot-driving-modeld \
  --catalog /path/to/native-catalog --models /path/to/original-models --output /path/to/fresh-evidence
cargo test --manifest-path rust/Cargo.toml --workspace --locked
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
```

Local original artifacts use LLVM 20; CI compiles both original and exported
artifacts with LLVM 18. CI builds the daemon and runs the native comparison after
constructing the immutable catalog, retaining reports and logs. Workspace tests,
strict Clippy and formatting passed locally. Exact-head cloud/aarch64 and
inherited integration checks remain merge gates.

## Remaining runtime work

USB/eGPU discovery, loader timeout/retry, warm internal fallback and Jetlink
orchestration remain separate work under #1/#6. Manager selection, full startup,
route logging, existing upload and original-versus-Rust user comparisons also
remain open. No vehicle was accessed, installed or tested. Host CPU/native IPC
evidence and generic aarch64 builds do not establish QCOM device operation,
zero-copy behavior, timing improvements or CPU savings. The first device handoff
waits for the complete project-owned runtime candidate.

Docs-Not-Needed: internal daemon implementation and host comparison only;
production settings and user-facing behavior are unchanged.
