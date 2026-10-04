# Runtime composition validation (#195)

[PR #195](https://github.com/bin9208/openpilot-rust/pull/195) composes the
isolated Card, Selfdrived, Panda, camera, application UI and encoder candidates toward
the complete runtime gate in [#1](https://github.com/bin9208/openpilot-rust/issues/1).
It does not select the candidate for normal vehicle startup or establish device
acceptance. Required inherited checks and separate Rust/ARM gates remain active.

## First exact-head run and repairs

The first PR run on `8b8252df389422449f9f0e3ca87dd168b6e62947`,
[37127730571](https://github.com/bin9208/openpilot-rust/actions/runs/37127730571),
failed. The Card job passed, including the corrected startup/phase fixture fences;
that result does not override the other failed jobs. Raw logs and downloaded
artifacts remain in the main checkout's `.omo/evidence/runtime-195/`.

Four jobs exposed the same composition error: camera and Athena publishers and
the VisionServer test had not initialized the newly added VisionIPC `index`/`fd`
metadata fields. The publisher receives its actual buffer through `VisionImage`;
its metadata uses the selected index and `fd=-1`, while receivers obtain their
own imported descriptor. The actual VisionServer test verifies the received
index and descriptor along with the existing bytes, metadata and ownership
checks. Selected camera/Athena/UI/msgq all-target compilation, Clippy with
warnings denied and the actual VisionServer test passed locally after the fix.

The workspace formatting failure involved seven camera, native logging and SPI
test files. They now match the pinned Rust 1.94 formatter; no behavior change is
included in those formatting edits.

The Selfdrived failure checker assumed its first stderr line was startup
readiness. In the detached CI checkout, Git instead printed
`fatal: HEAD does not point to a branch` before the valid readiness line. The
checker now reads raw chunks until the marker or the original five-second
deadline, preserving all diagnostics and avoiding buffered text read-ahead.
Four focused tests cover the same-write prefix, delayed/split marker, EOF and
timeout. The exact CI daemon ELF, SHA-256
`6246c8b092d017828147593dbd3b18cf4859c5fe797a5631a11eb2a133df5a18`,
reproduced the old failure under an isolated detached Git view; the corrected
checker passed all nine failure/lifecycle cases under the same view. The
production daemon and Git diagnostics were unchanged. Evidence is in
`detached-red-v2/`, `detached-green-v2/` and
`selfdrived-detached-commands-v2.json` under the same evidence root. Earlier
private runner setup errors are retained separately.

The Panda SPI oracle emitted all 122 scenario results and then timed out at
120 seconds during LeakSanitizer teardown. Its globally wrapped `sched_yield`
allocated a JSON log entry while LeakSanitizer held its allocator lock. The
fixture now forwards yields outside its active loop to the real syscall.
ASan/UBSan/leak checking remain enabled, and the comparison timeout is unchanged.
With the same local compiler and original sources, the old wrapper also hangs
on empty input while the fixed wrapper exits normally. The full 122-scenario,
204-operation, 7,387-syscall matrix matches the retained Rust probe; all 338 yield
records and every original CI result are preserved. The inherited original
`spi.cc:713` UBSan warning remains visible and is tracked in
[#179](https://github.com/bin9208/openpilot-rust/issues/179); this is not a
sanitizer-clean claim for that source implementation.

The SPI checker now preserves partial stdout, stderr and timeout status on
failure. A real-child regression failed before this change and passes after it.
The final test allows two seconds for the child to start; this affects only the
regression fixture and does not change the 120-second oracle deadline.
The SPI investigation's 61-artifact receipt is
`panda-repair/receipt.json`, SHA-256
`44ed405ae138938dcf9550646663825c2bd9340d07ea26b1481e00be3630d4bc`.
The parent independently checked all recorded hashes. Its native probe is a
retained earlier ELF; a fresh current-source build remains part of hosted CI.

## Encoder composition and repeatable gates

The isolated encoder implementation was committed as `571a62cd` and merged with
the camera/UI transport additions in `51797878`. All 81 workspace members remain
present. Both synthetic encoder probes initialize the added VisionIPC metadata
fields; publication still uses its separately owned mapping/descriptor.

Composition exposed a test-only file-offset dependency: the UI-side descriptor
test had already read a payload through a duplicated descriptor, so the later
encoder ownership test started at the shared end position and observed EOF.
The test now reads the retained descriptor explicitly at offset zero. Original
descriptor sharing and runtime behavior are unchanged; the failure, syscall
trace, corrected CXX baseline and frozen binaries are preserved in the IPC
worktree's `.omo/evidence/native-ipc-194/baseline-*` records.

`check_encoder_ci.py` selects the JPEG and ZeroMQ native build outputs from the
current successful Cargo JSON stream, rejects missing or ambiguous selections,
checks original Python binding origins and records source/binary identities.
The new required `rust encoder runtime` job runs the complete original/native
host matrix and uploads failed as well as successful evidence. The existing
`rust aarch64 build` separately stages the exact reviewed FFmpeg/libyuv wheels,
checks their locked source identity and archive hashes, and builds the daemon
and both probes with static codecs and ION. It retains ELF headers, dependency
versions and hashes; this hosted cross-build is not a new AGNOS device result.

The parent exercised the complete new CI runner locally against fresh merged
executables: five codec cases/112 exact packets, 28 scripted V4L cases and ten
actual runtime cases/1,207 publications per implementation passed. Receipt:
`.omo/evidence/runtime-195/encoder-ci-host/receipt.json`, SHA-256
`186b3f8d4b9c7d0cd4b5e69105f4fbfb2dda23a163a0874deeced836c301b98a`.
The merged daemon SHA-256 is
`4e7a6f5ba6f8057bd1a853bd25888e55da4ebe57e4a0b882108c95b748b52794`.
Original wheel staging also passed locally with all 164 native files checked;
the Python package launchers are not installed. Eighteen focused staging,
current-build-selection and failure-detection checks passed. The first local
merge build encountered a CMake cache tied to the prior worktree; that cache was
preserved with its hashes before reconfiguration, separately from the compile
failure that exposed the two probe metadata initializers.

Shared transport/encoder/catalog tests, Rust Clippy and pinned formatting remain
required alongside the source comparisons. The complete
normal-startup/log-upload candidate, remaining process conversion and removal of
the project-owned C++ IPC implementation remain open. No C3X or other vehicle
was connected or tested.

The first push of the composed encoder head `9b692a5c` exposed an omitted update
to the CI routing test's strict expected dependency list. The production workflow
already required the encoder, but its guard still expected the prior list.
The guard now requires the encoder result too, exercises failure/cancel/skip/
missing-result propagation, and checks the actual encoder binding environment
and pinned ARM job. The original hosted failure and matching local failure are
retained; all 26 CI routing tests and eight inherited integration-policy tests
pass after the test correction. A new final-head hosted run is still required.

## Second exact-head run and repairs

The complete [PR run for `9b692a5c`](https://github.com/bin9208/openpilot-rust/actions/runs/37134532321)
passed the encoder, UI, Selfdrived and the other unchanged runtime jobs. Five
jobs failed and blocked the aggregate. Card and workspace both encountered a
second strict dependency-list assertion missing `encoder-runtime`; the assertion
now includes it, preserving exact required-job checking.

Panda reached its spidev comparison after the SPI repair, but its builder and
checker both used `oracles/spidev`. The checker deliberately requires a fresh
output directory and rejected the builder's existing directory. The builder now
owns `spidev-src`, while comparison evidence retains `spidev`. A regression
creates both directories through the real runner's dispatch and reproduces the
old collision before the repair.

Camera completed its comparisons and all eight runtime cases, then artifact
upload rejected colon-bearing failure-case directory names such as
`error-1-camera:271`. Both camera and sensor lifecycle checkers now percent-encode
only the evidence directory name. Original scenario names, failure selectors and
comparison rules are unchanged. Tests exercise the actual output-writing paths,
including a percent-bearing name that must not collide. They stub external
process execution and do not establish new camera runtime validation. The prior
hosted camera artifact was not uploaded; only its job log is retained.

The pinned encoder ARM release step failed with unresolved x264, zlib and VA
symbols. The local compiler wrapper had caused Rust to select LLD, whereas the
hosted cross-compiler name selected GNU BFD. A local BFD run reproduced the exact
missing symbols. Dependencies bundled into `openpilot-encoderd`'s rlib appeared
before `ffmpeg-sys-next`; BFD did not revisit them when FFmpeg introduced the
references. Merely disabling bundling did not change that order. The final
encoder link now passes the explicit static dependency archives after the Rust
rlibs, with a group for their mutual references and the GCC atomic support used
by pinned x264. The production codec sources, packages and runtime behavior are
unchanged.

All three encoder executables then built with GNU BFD in the pinned release
profile. The fresh codec executable, SHA-256
`c7c17061890c247e04dbd35fee485cf16365b8753173543048f806dc93ebc1fc`,
passed five source comparisons and 112 exact packets under the extracted AGNOS
loader. The parent verified every one of the 748 captured artifact hashes.
Receipt: `.omo/evidence/runtime-195/encoder-release-bfd-codecs/receipt.json`,
SHA-256 `2f1120bb1f95aa5619cbd8d47ffdfecf2535326a503ee453132a61ceca92424e`.
The initial local LLD success, failed BFD attempts, complete BFD build and all
three frozen executables remain separately identifiable in the evidence root.

The path/routing repair passes 15 focused tests, 26 CI routing checks and eight
inherited policy tests. Pinned workspace formatting also passes. These results
authorize a new exact-head CI run; they do not replace its required result or
the later full-startup/device acceptance gate.

## Static musl scheduler initialization

The [next PR run for `9fcf24de`](https://github.com/bin9208/openpilot-rust/actions/runs/37137697986)
passed every host runtime job, including the repaired Card, Panda and camera
jobs. The ARM job passed the encoder GNU BFD build and subsequent native daemon
builds, then failed in the final static musl workspace build: libc's musl
`sched_param` contains additional reserved fields, so the priority-only literal
did not compile. The raw failing job is `111245482634`.

Selfdrived now zero-initializes the complete libc structure before assigning
priority 53. The `/TICI` guard, FIFO scheduling, core 6 placement and syscall
error propagation are unchanged. A callback regression observes the actual
production initializer's PID, policy and priority without changing host
scheduling. An isolated proof crate includes the production module and retains
the exact old module as a separate failing control. With libc 0.2.189, the old
module reproduces the musl error and the new module passes the same target
check. The callback test passes natively and under pinned Miri's default,
strict-provenance and Tree Borrows configurations. These checks cover the
initializer; hosted CI still supplies the full static workspace build gate.

## Complete ARM job duration

The [PR run for `92d5b89e`](https://github.com/bin9208/openpilot-rust/actions/runs/37141988490)
passed all host runtime jobs and the aggregate Rust checks. The ARM job
`111258166725` completed the native GNU builds, then exceeded its 45-minute job
limit during the final static musl workspace build. The Actions annotation
explicitly records the timeout; it is not a completed static build or another
scheduler compilation error. Its raw log and annotation remain in
`92d-arm-111258166725.log` and `92d-arm-timeout-annotation.json`.

The ARM job now has a bounded 75-minute limit, tracked in
[#198](https://github.com/bin9208/openpilot-rust/issues/198). Build commands,
targets, assertions and artifacts are unchanged. A successful final-head run is
still required before integration; the timeout change itself proves no build.

## Selfdrived startup fixture readiness (#199)

Composing native Rust IPC exposed a startup assumption in the actual Selfdrived
checker. It stopped supplying inputs after the state topic became enabled and
expected the independently initialized events topic within its next 100 ms
receive. The retained CI CXX-backed daemon reproduced the same missing-event
failure. The checker now keeps supplying inputs within the existing ten-second
startup deadline until both the enabled state and a valid event list arrive.
Production code, payload assertions and runtime deadlines are unchanged. The
fixture defect is tracked in [#199](https://github.com/bin9208/openpilot-rust/issues/199).

The old checker failures remain in `ipc-selfdrived-cxx-control/` and
`ipc-selfdrived-v1/ipc/`. The repaired checker passes both retained CXX and fresh
Rust IPC binaries in `ipc-selfdrived-cxx-green/` and
`ipc-selfdrived-v1/ipc-v2/`: two starts, non-conflated bursts, Params refresh and
SIGINT joins each. All nine independent failure/lifecycle scenarios also pass
in `ipc-selfdrived-v1/failures/`. These are host transport-consumer checks;
remaining consumers and complete startup/upload are separate gates.

## Native IPC and navigation composition

Commit `1199fc53` composes the native IPC implementation `f148b6b6`; commit
`341350f4` additionally composes navigation `f404234b`. The parent reviewed the
IPC implementation and independently checked 158 retained hashes, 19 ELF symbol
and dependency audits, the ARM interoperability outcomes and the ION contract
reports. `ipc-parent-verification.json` and `REVIEW-ipc-f148b6b6.md` retain this
review. The project-owned transport is Rust; external ZeroMQ diagnostics and
other documented native library boundaries remain.

Fresh binaries from the composed sources are frozen in `ipc-consumers-v2/`,
`ipc-consumers-v3/` and `ipc-consumers-v4/`. Camera actual-main checks pass all
eight signal, missing/disabled camera, injected driver-error and publication
ordering scenarios using the retained original frame-state oracle. Results,
raw messages and hashes are under `ipc-consumers-runtime-v1/camera-*`.

The encoder passes all ten original/native runtime scenarios against those
fresh transport libraries, including all seven profiles, multiple streams,
receive-only restart and lag. Its receipt is
`ipc-consumers-runtime-v1/encoder/receipt.json`, SHA-256
`80bdba61f9c15ea87bf91b9ee1e8332ef0123e909fa10b45195be3494ca233d1`.
UI verification passes four actual X11/IPC/input/recording/cleanup scenarios,
compact and large settings/overlay/dialog interactions, and original/native
driver/navigation camera rendering. Its receipt is `ipc-ui-v1/receipt.json`,
SHA-256 `b7f95fab9dfbce072586c7a9cfd013f2d9ad2a8a27cd0e8d98d9c80d15bd4622`.
The first local launch lacked Xvfb on PATH; the successful run uses the already
staged Xvfb and its libraries. Earlier build-environment failures remain in
`ipc-consumers-v1/` and `ipc-consumers-v2/encoder/`.

Card and both model daemons also compile with the new IPC library; their fresh
consumer replay remains separate from that compilation claim. Navigation passes
its complete host/GNU ARM comparisons and five actual IPC scenarios through the
extracted AGNOS loader, as recorded in
[navigation validation](rust-navd-196.md). All of these remain intermediate
component/composition evidence before normal startup and existing log upload.

## Native IPC musl correction and consumer replay (2026-10-04)

At `0bce1eb06b45688e67f6af7262ff63934b1befcf`, all 23 other Rust jobs and
the inherited integration gate pass. The [ARM job](https://github.com/bin9208/openpilot-rust/actions/runs/37148293326/job/111276644934)
passes GNU ARM consumers and fails the static musl workspace build: musl's
`cmsghdr.cmsg_len` and `msghdr.msg_controllen` are narrower integer types than
GNU's `usize` fields. Checked conversions now isolate the socket ABI while
length arithmetic and indexing remain `usize`. Polling retains its existing
timespec values without naming libc's deprecated musl `time_t` alias.

The first static ARM execution also exposes uninitialized padding in the C++
ABI fixture's `VisionBuf` record between `fd` and `width`. The fixture now
explicitly initializes the complete representations of its trivially copyable
records before setting fields. The full serialized-byte assertion is retained.
Production reference sources and Rust wire layout are unchanged.

Fresh checks pass: 46 host tests, strict all-feature Clippy, all nine static
ARM test executables under QEMU (including original C++ peers), the host suite
with Rust ASAN and C++ ASAN/UBSAN, and 21 pure memory tests at all four Miri
levels including strict provenance and Tree Borrows. Miri does not execute the
native socket boundary; the real socket tests and sanitizer run cover it.
Receipts under the private `runtime-195/ipc-musl-fix/` directory are:

| Receipt | SHA-256 |
| --- | --- |
| `host-v2/receipt.json` | `0a375d00d5026efc175ffad7d175984b051a7f34246952931006483ff3003cd0` |
| `musl-v3/receipt.json` | `8d452bcd0cf317829913ca4c8b6492a41f9b0a37fd28fb81a3658861ef7ef8f2` |
| `asan-v1/receipt.json` | `830319a35f4d8596e2346c01e83dca9bb06f3ebdb1986ab4b3b1b07adaf1857f` |
| `miri-v1/receipt.json` | `0c7d51410eb15cd03c51264a732cfa1106c5bf87a0157cd94d2e62d3a5d9dd8c` |

Fresh native-IPC consumer replay also passes the Card constructor, independent
100 Hz CAN/control input, complete wire output, Params persistence and shutdown
checks for 19 scenarios and 1,520 paired steps. All 38 source/native peers exit
with the expected statuses. `ipc-card-v2/result.json` has SHA-256
`281062fbcb6ec5e5a5a553ebee7a2917b5cbd165b304dee35d31fec45da6538b`.
An earlier source Python Nissan peer exceeded the five-second shutdown timeout;
its completed KeyboardInterrupt traceback and raw evidence are preserved.
The cause remains unresolved in [issue #201](https://github.com/bin9208/openpilot-rust/issues/201): narrow repeats and the complete fresh sweep do
not reproduce it. No source shutdown code, timeout, or assertion was changed.

Both model consumers pass startup plus actual original-camera/message
comparisons using freshly frozen native-IPC executables. The separate
`ipc-models-v2/receipt.json` SHA-256 is
`280eb31789e3e70cefb6500194dd225f02902e2fd33fe0a175080d707892d278`.
That focused run does not select the optional log collector. Full normal
startup/upload composition and user device acceptance remain outstanding.

## RadarCAN, planning and Xiaoge composition (2026-10-04)

Commits `4993bf4a`, `a081c6ee` and `cb158f14` compose the RadarCAN, planning and
Xiaoge candidates. Workspace/lockfile and strict CI dependency conflicts retain
all components. Required jobs now include original/native RadarCAN host and ARM
comparisons, planner host and native ARM comparisons, Xiaoge host and native ARM
comparisons, and each component's memory checks. Hosted exact-SHA results for
this composition remain pending; the earlier `0697eff6` CI success covers its
earlier component set only.

The parent independently reviewed the runtime boundaries and reproduced Params
read-error differences in the three consumers. Scoped corrections under
[#205](https://github.com/bin9208/openpilot-rust/issues/205) preserve the shared
Params API and original empty defaults/recovery. Planner additionally restores
the original SIGTERM disposition after CarParams arrives. Its corrected host
checks pass 29 package tests, 13 real source/native lifecycle scenarios and four
IPC runs with 121 exact ordered publications each. Its ARM correction builds,
passes the Params regression under the extracted AGNOS loader, and resolves all
12 native dependency/version entries. These are local/emulated checks.

The new planner CI recipe also passes all 12 checks locally, including complete
owners/main, three policy variants, solver fault recovery and the real daemon.
Generated-C ASan/UBSan ABI, owner and fault checks pass separately. Rust and the
pinned external numerical libraries are not claimed instrumented by that run.
Xiaoge's local CI recipe passes policy, lane postprocessing, full inference,
OpenCV/JPEG, actual HTTP/TCP, live IPC and lifecycle; retained ARM comparisons
use the actual original ARM Python/NumPy/OpenCV execution. External OpenCV and
JPEG instrumentation remains separate from the pure Rust Miri checks.

Detailed artifacts, source provenance and limitations are retained in
[RadarCAN](rust-radarcan-193.md), [planning](rust-plannerd-197.md) and
[Xiaoge](rust-xiaoge-200.md). Carrot navigation #206 and radar fusion #207 are
still in progress. The remaining registered services, final packaging and
complete ordinary startup/log upload remain open under #1. No device comparison
is requested at this intermediate stage.
