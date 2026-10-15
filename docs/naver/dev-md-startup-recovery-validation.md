# dev-md startup recovery port

Source: `ajouatom/openpilot:carrot-wip`
`e8e152dbcfd9574a1791ed12026d1880e87689b3`,
`openpilot/common/startup_recovery.py` and
`openpilot/system/ui/startup_recovery.py`. Issue #338, `dev-md` only.

## Current evidence

The native policy core in `rust/crates/startup-recovery/src/state.rs` passed
5,120 sequential transitions from the actual pinned Python class on both HOST
and QEMU ARM. All state/detail strings, pending reboot identities, busy admission,
callback order and lock-release paths match. Cases include manual/automatic
updates, same-revision waiting, a failed reboot followed by retry, rebuild/update
exclusion, lock/network/request/reboot errors, and the last 2,000 Unicode
characters of an error. Host strict all-target Clippy passed.

`rust/fixtures/startup-recovery-state-final.json` records 160 sequences of 32
operations. The source oracle runs the original class with controlled repository,
lock and reboot boundaries; it does not access a vehicle or modify this checkout.
Receipts: `.analysis/scratch/2026-10-15-dev-md-resume/recovery-core/state-*`.

The bounded log reader/capture and all 160 log / 48 byte-forwarding source
cases passed on HOST and ARM, including strict Clippy. These include ANSI escape
removal, invalid UTF-8, Unicode line boundaries and diagnostics, the last 64 KiB,
whole-chunk retention, exact forwarded bytes and periodic/final writes.

The native repository/package backend also passed seven owned transaction cases
on HOST and ARM: new update staging, unchanged automatic waiting, manual retry,
tracked local edits, lock contention, staging failure, and package repair. The
actual Git operations use an owned bare repository redirected from the allowed
Rust origin; installer/audio/reboot recipients are fake executables. Active HEAD
and tracked source remain unchanged during staging. The stage recipient checks
that the shared repository lock is held. The ARM test uses the ARM process-child
handshake through an explicit QEMU wrapper. Strict all-target Clippy passed.

This backend adapts source updates to the existing verified source/package
installer. `Rebuild` prepares a native package repair before reboot; it does not
remove SCons caches or create a Python build. This is an explicit native behavior
adaptation, supported here at the transaction boundary and by the earlier 46
native installer cases. The live display and launcher are now wired as described below; final runtime assembly remains pending.

## Isolated action and display ownership

`carrot-recovery __startup-action` is now registered as an internal command. It
rejects calls without the failed-startup marker, invalid pending revisions and
automatic rebuild requests before starting work. The action sets noninteractive
Git in its own process, atomically publishes bounded state, preserves the pending
revision across a failed reboot, and refuses a real workstation reboot.

The owned action worker and CLI passed HOST and ARM tests: real unchanged-update
completion, duplicate exclusion, unexpected child exit, and killing/reaping a
TERM-resistant session descendant on owner stop. ARM tests execute the same ARM
code; the native `qemu-subreaper` exec supervisor sets and verifies the Linux
subreaper attribute because QEMU does not emulate that prctl. The test wrapper
copies its ARM executable into the owned helper directory so `current_exe()`
finds the correct sibling. No production admission/path check was bypassed.
Strict all-target Clippy passed for recovery, recovery core and startup UI.

The recovery layout passed 84 actual original/native rendered frames across
536x240 and 2160x1080, all seven states, long paths, Korean/Unicode text, wrapping
and multiple pages. Every RGBA byte and page/hit-area metadata value matches.
The native view example links for ARM; graphics execution was on the HOST Xvfb
surface, not on a vehicle or ARM GPU. Independent synthetic font-metric layout
cases also passed HOST/ARM. Render receipts are `recovery-core/render-all/` under
the current scratch record; fixture is `rust/fixtures/startup-recovery-layout-final.json`.

## Live display and launcher integration (2026-10-15 continuation)

The registered `openpilot-startup-recovery` binary now owns the actual Raylib
loop, evdev/mouse press detection, page navigation, automatic checks and one
isolated action worker. The launcher selects it after releasing the repository
lock. A display exit triggers the same binary's headless automatic action, then
a 30-second wait before another display attempt. Preview exits before action,
touch-log, network-monitor or display-power side effects. Non-preview operations
require the failed-startup marker. SIGINT/TERM stops and reaps the action session.

HOST Xvfb integration ran the real binary with owned command recipients: initial
5-second automatic check, an actual XTest mouse press on the update button,
manual action, clean SIGTERM, and separate headless execution all passed. Both
preview sizes passed without touching action or touch-log files. The GUI receipt
is `recovery-core/app-live/result.json`. This executes real HOST graphics and
processes; it does not execute ARM graphics, a real update or a device reboot.

Raw touch history passed 3,072 source frame transitions on HOST/ARM, including
slot-zero handling and real nonblocking FIFO/read-error fallback. TouchLog passed
768 original filesystem operations on HOST/ARM, including Unicode JSON, rotation,
fsync and non-blocking log failures. The source retry policy passed 4,096 loop
steps; the additional action/clock ordering integration test also passed HOST/ARM after
correcting the call boundary so the retry deadline is sampled after worker start.

Backlight writes/device dimensions passed 160 original-source cases on HOST/ARM.
The new case corpus first exposed a CRLF difference: Python Path.read_text uses
universal newlines before stripping device-model terminators. Rust now preserves
that source behavior. Native repair wording reflects verified package staging
before reboot, instead of promising a Python/SCons build after reboot. 168 actual
rendered comparisons passed exactly: 84 unchanged source layouts and 84 source
layouts composed with these explicit native wording changes, both sizes. All
RGBA bytes, wrapping, page metadata and button areas match the corresponding
reference. `recovery-core/render-current/result.json` records binary and pixel hashes.

Manager stdout/stderr now flow through the source bounded 64-KiB capture policy,
while the launcher's result remains the manager's exit status. On child exit the
reader drains available bytes and stops even if another process retains a writer;
a continuously writing inherited producer gets at most 250 ms to drain. The
retained-writer/partial-line test passed HOST/ARM. A real failed-manager bootstrap
scenario verifies preserved exit 7, complete forwarded output, a bounded tail,
original final error, GUI-to-headless fallback and no inherited repository lock.
All 14 bootstrap scenarios pass on HOST and ARM. The ARM launcher is real ARM
code under QEMU with owned HOST command recipients; the separate worker gate runs
actual ARM action/handshake code. Neither is a complete runtime boot.

The binary is required by launcher verification, sealer and runtime installer.
HOST validation passed sealer9, installer8-library+46-CLI, launcher-package2,
recovery UI4, capture1, and the seven real recovery transaction/CLI/lifecycle
scenarios. ARM UI4/capture1 and recovery transaction/CLI/lifecycle tests passed;
the real UI and launcher binaries link for ARM. Six-crate strict all-target
Clippy passed after the final schedule call-order adjustment.

## Final connected corrections and offline repair

The source test inventory exposed the inherited TextWindow wait contract still
waiting only for exit 1. A bounded RED reproduced exit 0 polling again; the fix
now ends for all exits. Exact source cases 0, 1, 2 and -9 passed HOST/ARM.
`openpilot-startup-recovery --capture-log PATH` also exposes the native streaming
capture command without requiring the failed-startup marker: 48 source byte
sequences passed on each architecture, with exact forwarded bytes and final
retained-tail SHA-256.

The display and isolated reboot callback share the touch log across processes.
The original thread mutex is retained for local serialization, and runtime
loggers additionally use a bounded 100-ms file lock for append/rotation. A held
logger cannot indefinitely block recovery; contention drops that diagnostic.
The distinct-instance rotation/timeout test passed HOST/ARM alongside all 768
unchanged single-logger source cases. This is an explicit native isolation
adaptation, not a claim that Python used an interprocess lock.

An actual offline repair test first failed because package repair required a
network download. The installer now retains the verified compressed archive in
its owning generation; existing space reservation includes its bytes, and normal
generation cleanup also removes it. Same-source staging can use that cache only
after checking regular files, full source identity, generation identity, exact
archive length and SHA-256. Normal startup still verifies the executable manifest;
the cache is non-executable and is reverified before repair. Older generations
without an archive remain valid but require a download for repair.

The new offline test corrupts the active manager, denies every download, stages
and atomically activates a verified same-SHA repair, and confirms the active
source/files remain untouched before activation. Five rejection cases cover
corruption, missing cache, symlink cache, invalid descriptor and another source
revision. Invalid cache falls back to the configured downloader, which this
fixture rejects; no partial candidate is published. Installer HOST8-library +
48-CLI and ARM48-CLI passed, with strict Clippy. ARM CLI fixtures use an explicit
test-only QEMU launcher for their real ARM installer children; production has no
launcher override. Receipts: `recovery-core/offline-*`.

## Remaining integration

These source components are implemented and independently gated; whole-runtime
assembly, exact-SHA CI and user hardware acceptance remain separate open gates.
The native backend retains verified source/package staging and its shared lock;
it never merges unverified source into the active runtime or recreates SCons/Python
builds. No device deployment or main/dev integration is authorized by this work.
