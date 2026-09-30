# Independent Rust runtime port record

The active port belongs to `bin9208/openpilot-rust`, issue
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance and the
approved complete-runtime scope are recorded in [the design](../rust-port/design.md).
This repository does not synchronize or publish changes to the original fork.

Native model execution is tracked in
[#6](https://github.com/bin9208/openpilot-rust/issues/6), with the
[artifact plan](../rust-port/model-artifact-plan.md) and
[validation ledger](../rust-port/model-artifact-validation.md).
CPU library/model comparisons are intermediate implementation evidence. The user
requested the first device handoff only after the whole runtime can start, log
and use the existing upload flow for comparison with the original implementation.

The [native QCOM ledger](../rust-port/qcom-runtime-validation.md) records the
kernel/argument/packet oracle, KGSL ABI and lifetime checks, and remaining GPU
acceptance. Backend code and host tests do not complete the full runtime gate.

[VisionIPC issue #14](https://github.com/bin9208/openpilot-rust/issues/14) adds
the Rust-owned camera client boundary over the external msgq library. The
[boundary ledger](../rust-port/visionipc-plan.md) records native-server
interoperability, reconnect, copied-frame lifetime and sanitizer evidence.
Camera/model daemon integration and GPU imports remain full-runtime work.

[Model output issue #16](https://github.com/bin9208/openpilot-rust/issues/16)
ports model interpretation, action calculation and the four original cereal
publications. The [validation record](../rust-port/model-output-validation.md)
documents original-function comparisons, history/discrete-decision boundaries,
architecture-specific exponential rounding and remaining daemon integration.

[Desire state issue #24](https://github.com/bin9208/openpilot-rust/issues/24)
ports lane/turn intent, side obstacles and Bluetooth command consumption. Its
[validation record](../rust-port/desire-validation.md) compares complete state
sequences and actual command journals while retaining the original gates.
This library still requires connection to the driving daemon.

[Driver daemon issue #19](https://github.com/bin9208/openpilot-rust/issues/19)
connects driver VisionIPC, native inference, calibration and `driverStateV2` in
a continuous Rust process. Its [validation record](../rust-port/driver-daemon-validation.md)
documents both camera resolutions, original-model/message comparisons and
remaining QCOM input, manager and full-runtime acceptance work.

[Message-state issue #26](https://github.com/bin9208/openpilot-rust/issues/26)
preserves shared subscription freshness, validity and frequency contracts. Its
[validation record](../rust-port/messaging-validation.md) covers source state
comparisons, original service capacities, native collective polling and ownership.
The source catalog/schema inconsistency is separately tracked in #27.

[Driving input issue #20](https://github.com/bin9208/openpilot-rust/issues/20)
ports camera pairing, drop state, calibration and packed policy inputs. The
[comparison record](../rust-port/model-input-validation.md) includes original
source sequences and exact native camera/model/recurrent output comparisons.
Complete driving daemon orchestration remains tracked under #1/#6.


[Monitoring policy issue #29](https://github.com/bin9208/openpilot-rust/issues/29)
ports awareness, distraction, calibration, fallback, lockout and complete
`driverMonitoringState` messages. Its [validation record](../rust-port/monitoring-validation.md)
compares 209,144 original-policy frames and packets. Continuous IPC and
whole-runtime integration remain open.

[Internal driving daemon issue #31](https://github.com/bin9208/openpilot-rust/issues/31)
connects camera pairing, calibration, native inference, action/desire state and
the three original driving publications in a continuous Rust process. Its
[validation record](../rust-port/driving-daemon-validation.md) covers original
model/loop comparisons over real IPC and remaining external-inference integration.

[Continuous procLog issue #30](https://github.com/bin9208/openpilot-rust/issues/30)
connects the validated collector and canonical encoder to the original 0.5 Hz
runtime publication loop. Its [validation record](../rust-port/proclog-runtime-validation.md)
includes source deadline comparisons, real IPC, overrun recovery and shutdown.
Manager selection and whole-runtime device acceptance remain pending.

[Monitoring daemon issue #38](https://github.com/bin9208/openpilot-rust/issues/38)
connects the validated policy to original driver-frame polling, validity/demo
gates and post-publication Params updates. Its [validation record](../rust-port/dmonitoring-daemon-validation.md)
includes full source-loop/native-message comparisons and handedness persistence.

[Calibration daemon issue #35](https://github.com/bin9208/openpilot-rust/issues/35)
ports the calibration block estimator, source rotation math, `liveCalibration`
publication and asynchronous `CalibrationParams` persistence. Its
[validation record](../rust-port/calibration-validation.md) covers long original-class
histories, original main-loop validity/cadence, native Python IPC, cached values,
Float32 Params parsing and signal handling. Manager selection and the complete
startup/logging/upload/device gate remain pending.

[Diagnostic log collector issue #34](https://github.com/bin9208/openpilot-rust/issues/34)
adds the continuous Rust ZeroMQ collector, source-compatible Swaglog formatting
and rotation, and original `logMessage`/`errorLogMessage` publications. Its
[validation record](../rust-port/logmessaged-validation.md) separates source
byte comparisons, native transport/error-path checks and the CPython recursion
resource difference. External libzmq/msgq remain explicit; manager selection,
route logger integration and the complete startup/upload candidate remain open.

[Jetlink runtime issue #44](https://github.com/bin9208/openpilot-rust/issues/44)
adds native owner/provisioning policy, FunctionFS protocol, RPC worker and driving
source-selection integration. The [validation record](../rust-port/jetlink.md)
separates original state/parser comparisons, actual Unix-socket interoperability,
FIFO transport tests and driving regressions from still-unrun device acceptance.
Parent reruns confirm the source decisions and both RPC directions. Existing
validation/loss latches remain required; no real gadget or vehicle is used.

[Structured logging issue #45](https://github.com/bin9208/openpilot-rust/issues/45)
adds native per-thread producers, scoped/global context, source-compatible
records and bounded runtime timing/communication summaries. Its
[validation record](../rust-port/logging-client-validation.md) covers exact source
bytes and numeric types, original/Rust collectors, actual OS identity, fork
reconnection and backpressure. Integration into each daemon's original callsites
is a subsequent full-runtime requirement; this library does not establish that
all Rust process diagnostics reach a route or upload.

[Torque estimator issue #40](https://github.com/bin9208/openpilot-rust/issues/40)
adds the continuous native torque estimator, original histories/RNG/SVD policy
and asynchronous cache persistence. Its [validation record](../rust-port/torqued-validation.md)
records the correction to source-locked NumPy 2.5.3/OpenBLAS, same-architecture
host/ARM source comparisons and real IPC/Params shutdown behavior. Parent reruns
confirm the original estimator and native lifecycle. All existing model pipeline
artifacts still require regeneration and exact-head CI under the corrected
dependency. Generic builds do not establish vehicle or CPU acceptance.

[Model diagnostics issue #53](https://github.com/bin9208/openpilot-rust/issues/53)
connects original model-daemon log callsites and driving runtimeTiming to the
Rust producer/collector path. The [validation record](../rust-port/model-diagnostics.md)
separates exact source-expression checks, real model/VisionIPC publications,
transport fault handling and source-locked NumPy 2.5.3 artifacts. Driver monitoring
does not gain timing events absent from its source. The existing model-loading
order differs from Python and remains an acceptance gap under
[#55](https://github.com/bin9208/openpilot-rust/issues/55); first-loop initialization
time is retained, and still-unported eGPU callsites stay explicit. Driver-monitoring
startup order is separately tracked in [#60](https://github.com/bin9208/openpilot-rust/issues/60).
