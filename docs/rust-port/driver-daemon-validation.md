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

## Queue-capacity regression and repair (2026-09-30)

Issue #19 was reopened after source integration exposed that both Rust sockets
and the original QA peer had used 1 MiB defaults. The actual
`openpilot/cereal/services.py` definitions require 256,000 bytes for each of
`liveCalibration` and `driverStateV2`. The earlier matched-peer test therefore
did not establish interoperability with normal source service queues.

The daemon now obtains both capacities from `openpilot-messaging::services`,
using a catalog-only dependency. The QA peer independently compiles a header
from the original Python `build_header()` and passes each source capacity to
its native socket constructor. After the Rust daemon connects, the harness
checks both actual shared-file lengths minus the native msgq header size
against Python `SERVICE_LIST`; it records those capacities with every run.

The base `277964e35f3a12037d143288d5641db3ed90e240` binary failed this corrected
peer scenario before the repair: its first camera frame reached model loading,
then it exited with `existing msgq queue has incompatible size or type`.
After the repair, all four original daemon scenarios passed (1344x760 and
1928x1208, each with raw output enabled and disabled). Both queues measured
256,000 bytes in every run. Ten publications / 470 fields matched the original
model oracle; calibration retention/broadcast/rejection, camera/frame policy,
no-frame timeouts and signal shutdown checks remained enabled.

Host invocation, with the original compiled models and native pipeline catalog:

```sh
PYTHONPATH="$PWD:$PWD/tinygrad_repo:$PWD/rust/tools" \
  DEV=CPU:LLVM LLVM_PATH=/usr/lib/x86_64-linux-gnu/libLLVM-20.so \
  CPU_COUNT=2 JIT=1 JIT_BATCH_SIZE=0 python rust/tools/check_driver_daemon.py \
  --binary rust/target/debug/openpilot-dmonitoringmodeld \
  --catalog /path/to/native-pipeline-catalog --models /path/to/original-model-build \
  --output /path/to/driver-queue-evidence
```

Local artifacts in the issue worktree: `.omo/evidence/driver-queue-fix/`.
`red.log` and `red/1344-raw-1/daemon.log` capture the old binary failure;
`green/report.json` and per-run reports/captured packets prove the corrected
native interoperation and capacities. `tests.log`, `clippy.log`, `ruff.log`
and `INDEX.json` record focused verification and exact invocations. The existing
Rust workflow already runs this complete driver harness; exact-head cloud CI,
independent review and post-merge verification remain integration gates.

Docs-Not-Needed: internal runtime IPC compatibility repair; no settings,
production selection or user-facing behavior change.

## Remaining acceptance

Host CPU results do not validate QCOM execution, AGNOS scheduling, camera ION
imports on hardware, or CPU savings. This implementation copies camera bytes
into owned staging memory and then the model backend; zero-copy QCOM input
integration remains pending. Production daemon selection is unchanged. Driving
model orchestration, other project-owned services, normal startup/logging/upload
and original-versus-Rust device comparison remain under the full-runtime gate.
The first user device test follows that complete candidate, not this increment.
