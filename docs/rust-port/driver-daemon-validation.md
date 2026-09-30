# Rust driver-model daemon

Issue [#19](https://github.com/bin9208/openpilot-rust/issues/19) connects the
native model runtime and model-output port to the actual driver-camera loop.
This is an intermediate part of the complete runtime in
[#1](https://github.com/bin9208/openpilot-rust/issues/1), not a device handoff.

## Runtime contract

`openpilot-dmonitoringmodeld --trusted-catalog PATH` runs continuously.
`--frames N` optionally bounds successful publications. The catalog contains
trusted immutable native executable artifacts and selects the driver pipeline
using the camera's dimensions. Supported backends are CPU Clang/LLVM and QCOM.
The process itself does not invoke Python or unpickle model artifacts.

The daemon connects to `camerad`'s conflated driver stream, loads the matching
1344x760 or 1928x1208 pipeline, consumes `liveCalibration`, runs the original
prepare/model stages, and publishes `driverStateV2`. Camera layout must match
the catalog's stride, UV offset and full NV12 buffer length. Fixed transforms
are the float32 results of the original camera/model intrinsic inversion.
Calibration starts at zero; updates retain the original three-value assignment
and one-value broadcast, including updates whose Event valid flag is false.
Invalid calibration lengths terminate the process instead of partially updating
state. Frame validity does not suppress the publication, matching the source.

`SEND_RAW_PRED` follows Python's nonempty-string behavior, including `0`.
On TICI the process requests core 7 and FIFO priority 5, as the original driver
daemon does. SIGINT/SIGTERM stop the loop and release resources. An absent camera
is retried every 100 ms; receive timeouts do not publish. A changed camera server
retains the original no-auto-reconnect behavior. Manager restart integration is
still pending.

`Publisher::for_runtime` and `Subscriber::for_runtime` explicitly use the original
namespace, including an unset `OPENPILOT_PREFIX`'s flat `/dev/shm/msgq_ENDPOINT`
path. Existing probe constructors continue requiring `rust-probe-NAME`. Endpoint,
capacity, namespace, existing-file and Rust publisher-lock checks still apply.
Original native publishers do not participate in the Rust publisher lock;
manager selection must prevent two producers for the same service.

## Observed host verification

`rust/tools/check_driver_daemon.py` builds a C++ peer against the original msgq
and VisionIPC sources. A separate Rust daemon consumes actual shared camera
buffers and publishes actual cereal messages. Only the test oracle uses Python.
It executes the original compiled driver/warp artifacts and the original
parse/publication functions, with transforms computed from source intrinsics.

The September 30 host run passed both resolutions with raw prediction enabled
and disabled: ten compared publications and 470 fields. Eight raw publications
matched all 553 float32 outputs byte for byte. Parsed floats use the established
1e-6 absolute/relative bound; IDs, validity and other discrete values are exact.
Execution durations and monotonic timestamps are range/order checked, not
compared to the oracle's elapsed time.

Scenarios cover startup before the camera is available, initial zero calibration,
invalid-flag updates, retained calibration, one-value broadcast, malformed
calibration termination, no-frame/no-publication timeouts, SIGTERM while waiting
for frames and SIGINT while waiting for camera connection. The native subscriber
is settled using a startup frame because original msgq resets existing readers
when a publisher initializes. This does not change production queue semantics.

The calibration tests and runtime-namespace tests were observed failing before
their implementation. Focused Rust tests, workspace Clippy and the native daemon
comparison pass. The workflow repeats the actual daemon comparison alongside
the original model pipelines and cross-builds the daemon with the ION feature.
The independent review also reproduced valid calibration bytes failing when
the input slice was not 8-byte aligned. The decoder now uses owned aligned
Cap'n Proto segments; offsets 0 through 7 pass, including symbolic-alignment
Miri. The workflow preserves this regression check. Exact commit review and
Actions results are recorded on #19 and its PR.

## Remaining acceptance

Host CPU results do not validate QCOM execution, AGNOS scheduling, camera ION
imports on hardware, or CPU savings. This implementation copies camera bytes
into owned staging memory and then the model backend; zero-copy QCOM input
integration remains pending. Production daemon selection is unchanged. Driving
model orchestration, other project-owned services, normal startup/logging/upload
and original-versus-Rust device comparison remain under the full-runtime gate.
The first user device test follows that complete candidate, not this increment.
