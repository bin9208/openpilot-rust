# Runtime composition validation (#195)

[PR #195](https://github.com/bin9208/openpilot-rust/pull/195) composes the
isolated Card, Selfdrived, Panda, camera and application UI candidates toward
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

The repaired head still requires new exact-head Actions results. The complete
normal-startup/log-upload candidate, remaining process conversion and removal of
the project-owned C++ IPC implementation remain open. No C3X or other vehicle
was connected or tested.
