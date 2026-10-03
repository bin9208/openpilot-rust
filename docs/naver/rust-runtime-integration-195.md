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
