# Model daemon logging and runtime timing

Issue [#53](https://github.com/bin9208/openpilot-rust/issues/53), under full runtime
port [#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance:
unchanged `openpilot/selfdrive/modeld/modeld.py`, `dmonitoringmodeld.py`, and
`openpilot/common/runtime_diagnostics.py`. The implementation uses the
[#45 logging producer](logging-client-validation.md); no Python runs in either
Rust daemon.

The driving daemon now emits the source startup, selected camera streams,
connection dimensions, model load, CarParams, unavailable-pair, dropped-frame
and SIGINT messages with their original severities. Driver monitoring emits its
four original connection/load/SIGINT callsites. Existing unstructured progress
messages are replaced by these records. Neither source daemon invokes
`communication_snapshot`; driver monitoring does not instantiate
`RuntimeDiagnostics`. No communication fields or driver timing events are added.

The connection-layout getter reads the original VisionIPC client's first mapped
buffer metadata without polling or consuming frames. It returns `None` when no
buffers exist and copies only width, height, stride, UV offset and byte length.
The existing import checks still validate layouts. No camera slice or native
pointer crosses this new interface; copied scalars outlive the connection.

## Recorded phases

Driving `runtimeTiming` retains the original one-second interval, metric order,
mean/max/count types, scheduler deltas and context keys. Every processed camera
pair contributes, including prepare-only frames; unavailable pairs produce the
source debug message and no timing sample.

| Metric | Actual Rust interval or observation |
| --- | --- |
| `camera_wait_ms` | Loop start before parameter refresh through completed pair reception/copy |
| `camera_age_at_run_ms` | Monotonic time at inference entry minus the received EOF timestamp |
| `inference_ms` | Existing model execution interval, including Jetlink begin, native inference, finish and existing state persistence |
| `inference_thread_cpu_ms` | Calling-thread CPU time around that inference region |
| `postprocess_ms` | Inference completion through postprocessing and all three publications |
| `loop_ms` | Loop entry through the point just before diagnostic aggregation |
| `thread_cpu_ms` | Calling-thread CPU time over the loop |
| `dropped_frames` | Existing drop tracker result, as an integer |
| `published` | Integer 1 when a model output exists, otherwise 0 |

Both CPU metrics use `CLOCK_THREAD_CPUTIME_ID`, not process CPU time or a wall
time approximation. Diagnostic aggregation and emission remain outside the
sampled loop tail. The source resets the aggregate before emission and suppresses
sink exceptions; the Rust integration preserves that boundary. Ordinary logging
calls continue to expose unexpected transport errors as the source does.
Queue-full sends remain nonblocking drops. SIGINT logs `got SIGINT`; SIGTERM does
not manufacture that message.

Context retains `backend`, `usbgpu` and `frame_id`. The explicit implementation
mapping is Python `ModelState` to Rust
`openpilot_driving_modeld::runtime::DrivingRuntime<'_>` from `type_name`.
`usbgpu=false` describes the currently internal model path, which stays warm
during Jetlink inference. It is not a placeholder for a future eGPU model path.
Catalog/backend choice and published Jetlink source/phase/execution fields are
unchanged. Records identify actual Rust callsites, PID/TID and build commit/tree;
they do not impersonate Python files or classes.

## Explicit remaining gaps

The startup-order differences discovered during this increment are tracked in
[#55](https://github.com/bin9208/openpilot-rust/issues/55) and
[#60](https://github.com/bin9208/openpilot-rust/issues/60). At the frozen #53
checkpoint, driving loaded after CarParams on the first pair and DM loaded after
its first frame. Their [startup follow-up](model-startup.md) moves construction to
validated connection dimensions before frame reception, with driving also loading
before CarParams. The #53 evidence retains the actual earlier first-loop cost;
follow-up evidence verifies the corrected order without subtracting time or
skipping timing samples. This does not establish whole-runtime readiness.

The following source callsites depend on still-unported eGPU paths and remain
pending: tmux-capture queue success/conflict/failure (modeld lines 59/63/68), USB
discovery grace/result and presence/compiled/requested status (234/239/241/244),
PCIe retry/load failure/timeout (301/305/313), runtime fallback (479), and first
eGPU publication completion (558). They are inventoried by the source-expression
oracle and are not fabricated from internal model or Params state. This increment
does not implement those paths or complete the startup/upload delivery gate.

## Verification and reproduction

Host evidence on 2026-09-30 uses an isolated NumPy **2.5.3** environment and newly
compiled original driving/DM artifacts, both camera warps and an immutable native
catalog. The driving ONNX is verified against its tracked LFS digest. Earlier
2.4.6 artifacts are not used as evidence for this increment.

- The actual source `diagnostics.record(...)` expression and original
  `RuntimeDiagnostics` match 2,053 deterministic inputs and 536 aggregates,
  including ordered keys, exact numeric types and Float64 bits. The backend type
  mapping above is explicit.
- Real native model/VisionIPC runs compare 90 driving publications/57,682 fields
  across both resolutions and road/wide/dual streams, plus 10 DM frames/470 fields.
  Raw prediction bytes, discrete fields and existing numerical tolerances remain
  unchanged. Original source log expressions validate message text, severity and
  applicable callsite conditions.
- The normal producer/collector path publishes real `logMessage` and
  `errorLogMessage` cereal packets and persists the expected disk records. Model
  timing events are present; driver timing events are absent.
- A link-only QA wrapper injects one `ENOTSOCK` or `EAGAIN` at the actual
  `zmq_msg_send` call for `runtimeTiming`. Each real model/VisionIPC run still
  produces 33 matching model publications/21,153 fields. The next timing interval
  contains exactly the post-failure frames. An ordinary-callsite fault exits
  nonzero, matching the original producer boundary. Production code has no fault
  injection option.
- Separate real queue saturation sends 5,000 records: original and Rust
  producers each accept 1,000 and drop 4,000. This measures the producer transport;
  the model fault tests above inject `EAGAIN` rather than claim 1,000 seconds of
  model timing traffic.
- The connection-layout test proves no frame consumption and scalar lifetime;
  native VisionIPC/msgq tests run under ASan/UBSan. Host tests/lints and the generic
  GNU aarch64 build remain separate from AGNOS/device execution.

With the original model environment and compiled original msgq Python binding:

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-driving-modeld --bins --examples --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-dmonitoringmodeld -p openpilot-logmessaged --bins --locked
python rust/tools/check_model_timing.py --binary rust/target/debug/examples/model_timing_trace --output /tmp/model-timing
python rust/tools/check_driving_daemon.py --binary rust/target/debug/openpilot-driving-modeld --collector rust/target/debug/openpilot-logmessaged --catalog /tmp/native-catalog --models /tmp/original-models --output /tmp/driving-logging
python rust/tools/check_driver_daemon.py --binary rust/target/debug/openpilot-dmonitoringmodeld --collector rust/target/debug/openpilot-logmessaged --catalog /tmp/native-catalog --models /tmp/original-models --output /tmp/driver-logging
python rust/tools/check_model_logging_faults.py --collector rust/target/debug/openpilot-logmessaged --catalog /tmp/native-catalog --models /tmp/original-models --output /tmp/model-log-faults
```

The Rust workflow repeats deterministic timing, actual model/collector and fault
checks against freshly generated source-locked pipelines. Parent integration and
exact-head CI remain required. Host timestamps and CPU samples are diagnostic
observations, not evidence of device CPU/thermal savings. No vehicle is contacted,
no production selector or scheduling policy changes, and the first device test
still waits for the complete runtime candidate under #1.

Docs-Not-Needed: model runtime diagnostics only; no user setting or production
selection behavior changes.
