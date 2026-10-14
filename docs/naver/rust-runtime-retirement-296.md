# Complete runtime retirement and post-drive optimization (#296)

> For agentic workers: execute the implementation stages continuously using
> superpowers:executing-plans; independent audits may use
> superpowers:dispatching-parallel-agents. Do not end at a queued CI run.

Goal: remove maintained Python execution/source dependencies, including
development tools, preserve existing functionality, resolve discovered defects,
and verify the complete source-matched candidate through normal device boot.

Spec: [full runtime design](../rust-port/design.md), with the user's explicit
2026-10-14 extension to developer tools and device startup verification.
Tracking: [#296](https://github.com/bin9208/openpilot-rust/issues/296).
Base: `f3446f65e1cbd894aba1aac7e724c12e6d89ff61`; prerequisite encoder fix
[PR #295](https://github.com/bin9208/openpilot-rust/pull/295),
`73a9e01709245de97ffb18b1a079b019bc3f9fd9`.

## Architecture and constraints

Native daemons retain their message contracts, priorities, periods, numerical
policies, settings and native library boundaries. Canonical data/assets must no
longer be extracted from Python source during Rust builds. Maintained build,
packaging, generation, documentation validation and test orchestration tools
must gain native implementations before their Python implementation is removed.
An external pinned original implementation used for comparison is still an
external validation dependency; moving a Python tool or wrapping its interpreter
does not count as converting that tool to Rust.

Only bin9208/openpilot-rust may be changed remotely. Preserve licensing, source
provenance, firmware/data/model assets and current user data. Private captures,
device identifiers and credentials remain outside Git. Host tests, target ABI,
offroad device operation, road acceptance and measured CPU savings are separate.
Before every build/install/large copy retain 25 GiB plus estimated growth; below
that floor recover 35 GiB before resuming. Use the coordinated target cache,
incremental compilation disabled, and bounded package builds.

## Initial inventory

Tracked `.py` files at the base candidate: 3,566. These comprise 1,011 under
openpilot, 1,261 Rust development/validation tools, 131 other project tools and
1,163 files of external/vendor provenance. Additional executable Python build
code includes 30 SConstruct/SConscript files and eight Cython source/declarations.
This is an inventory, not proof that all entries run on the device. Five AGNOS
Python services observed during the drive belong to the operating system;
the approved project's external OS boundary remains explicit.

## Implementation stages

- [ ] Finish the exact-SHA encoder regression, pre-merge CI and post-merge checks.
  Preserve the physical EOS reproduction and verify corrected full-runtime
  rotation, rather than treating the standalone source probe as acceptance.
- [ ] Remove source-data coupling in carrot-server/dashcam-upload build.rs,
  carrot-recovery embedded web assets, and qcomgpsd wire declarations. Compare
  old/new output bytes and compile with the original Python paths absent.
- [ ] Replace Python-dependent maintenance/startup entry points. Fix recovery
  origin switching and source/package activation sequencing (#297/#290), then
  verify missing-release failure leaves a bootable source/package pair intact.
- [ ] Fix concrete lifecycle/retention defects and optimize proven repeated work:
  watchdog stale PID and preservation-cache invalidation (#298), model input
  allocation reuse, zero-timeout msgq polling, persistent service generations
  (#287). Preserve packet, lifetime, shutdown and durability contracts.
- [ ] Finish module coverage, reproduce remaining SPI recovery and camera-client
  restart hazards, and implement bounded/correct recovery without relaxing
  health criteria. Audit remaining UI/connectivity/audio/telemetry wrappers.
- [ ] Port maintained developer tool families to Rust with equivalent positive,
  negative, incomplete-input and cleanup cases. Rewrite CI entry points to those
  tools; do not silently delete tests or count interpreter wrappers as ports.
- [ ] Remove replaced runtime/developer Python and Python build glue only after
  their consumers are native. Retain non-code external assets and licensing;
  document any external original-source validation dependency explicitly.
- [ ] Add tracked-file and runtime/package checks for Python source, shebangs,
  interpreter invocations, SCons and hidden Python/Cython dependency paths.
- [ ] Build/seal the complete AGNOS candidate, verify every ELF/provider and
  exact source identity, run all required CI and integration gates, publish
  the matching immutable package, and verify downloaded hashes.
- [ ] Apply the complete verified candidate while offroad, reboot through the
  normal service/startup path, verify same-generation native children, UI,
  recording/rotation and existing upload. Preserve rollback and user data.

## Review focus

1. Deleting a Python source file must not erase embedded web assets or wire data.
2. A failed package download must not strand a changed checkout without startup.
3. PID reuse and surviving old generations must not block startup or lose helpers.
4. Shared frame reuse must not alias pending camera frames or stale recurrent data.
5. Bookmarks written after a first scan must affect deletion priority.
6. A rewritten test/build tool must preserve failure detection and cleanup, not
   merely its happy-path output or number of files.

## Ledger

- Native documentation gate: `openpilot-dev-tools` / `check-user-docs` replaces
  the Python CLI and its test module; the required `check mapped user docs`
  workflow now builds and tests the Rust implementation. The prior 19 tests
  passed before retirement. Native tests cover all former scenarios plus Unicode
  glob matching, invalid mappings/options, missing base fallback, no-Python PATH
  and native exit status. Eleven native tests and Clippy `-D warnings` passed.
  A 125-case comparison matched exit code/stdout/stderr byte-for-byte before
  removing the replaced files. This is one developer-tool family, not completion
  of the full tool inventory.
- msgq zero-timeout poll: RED observed two queue checks and a changed signal
  mask inside the callback. The nonblocking path now checks once without signal
  mask or ppoll calls. All 52 msgq package tests and Clippy passed, including
  real original/native peers, interrupt timing and blocking lost-wakeup coverage.
  This removes work by construction; no whole-device CPU improvement is claimed.
- Model input ownership: the real VisionIPC test measured 40 large allocations
  for 40 warmed-up receives before the change and zero afterward. Fourteen
  driving-modeld integration tests and package Clippy passed, including retained
  snapshots, skew advancement and layout changes. No device timing claim.
- Source-data extraction removal: four build scripts pass with their former
  Python trees absent and an empty PATH. Five assets retain exact bytes; GPS
  declarations retain formatted Rust output and four upload constants retain
  values. Focused recovery/dashcam/GPS tests passed 30 with three intentional
  subprocess-driver ignores; server source-free tests passed four. Missing local
  process-child helper initially caused seven recovery fixture failures; building
  the native prerequisite resolved those failures.

- User explicitly confirmed that development tools are included in the Rust
  conversion. No scope reduction to only active Python processes is accepted.
- First source audits found hidden SCons runtime actions, Python-source build
  extraction, wrong recovery origin, stale watchdog/PID and preservation state,
  persistent generation hazards, and avoidable model-frame allocations.
- Ruling: parallel agents own disjoint source areas and request coordinated
  Cargo build slots; this avoids cache/build-environment interference.
- No all-module clearance, complete Python retirement, corrected-runtime device
  boot or CPU benefit has been established at this checkpoint.

Docs-Not-Needed: engineering plan and internal implementation evidence; setting
changes, if required later, receive their own paired user documentation updates.

## Issue #298: watchdog ownership and mutable bookmarks (2026-10-14)

Tracking: [#298](https://github.com/bin9208/openpilot-rust/issues/298).
The watchdog now treats its acquired exclusive lock as ownership authority;
a stale pidfile naming an unrelated live process no longer suppresses the web
server. The duplicate-lock and pidfile-cleanup protections remain intact.
The deleter reads current `user.preserve` attributes before ordering deletion,
so bookmarks added after a previous scan and subsequently removed bookmarks
both take effect. The exact `b"1"` marker, five-marked-segment limit, two prior
segments, boot/crash priorities and recording locks retain their policies.
The permanent cache is removed; the attribute probe uses a one-byte stack
buffer and treats longer values as non-markers without 64-KiB allocations.

RED evidence before implementation:
- `stale_pid_file_does_not_treat_an_unrelated_live_process_as_owner` failed with
  `stale PID prevented web server startup`, using an owned temporary `sleep`.
- `bookmark_added_after_a_scan_preserves_the_segment_and_two_prior` deleted
  `route--0` instead of `route--3` after the logger's synthetic late bookmark.
- `removed_bookmark_no_longer_defers_segment_deletion` deleted `route--1`
  instead of `route--0` after removing its marker.

GREEN: `cargo test --locked -p openpilot-runtime-launcher -p openpilot-deleter`
passed all 24 top-level tests; two ignored child fixtures were also executed
successfully by their parent handoff tests. Rustfmt and `git diff --check`
passed. Builds used two jobs, incremental compilation disabled and the shared
`/home/fma1/rust/cache/cargo-target`; available space stayed above 393 GiB.
The first build attempt failed before tests because `as` was absent from PATH;
prepending the existing native SDK bin directory corrected that environment.

The inherited Python xattr cache has the same bookmark defect. The interim
reference checker retains strict source comparison on unaffected scenarios;
`late-bookmark` and `removed-bookmark` independently assert both the inherited
outputs and explicit corrected outputs, and include both in their traces.
The corrected source oracle uses the same deletion algorithm with its xattr
cache refreshed before each scan. All 78 comparisons across 28 scenarios
passed against the Rust example. A negative control using the actual original
implementation as the peer was rejected at `late-bookmark`, confirming that
the correction is required rather than merely exempted. Python validation
orchestration remains pending the broader native-tool conversion.

These are temporary host files/processes only; no user routes, device runtime,
CPU savings or complete service-module clearance were established by #298.

## Issue #302: model camera generation loss (2026-10-14)

Tracking: [#302](https://github.com/bin9208/openpilot-rust/issues/302).
VisionClient already distinguishes an ordinary receive timeout from a changed
server ID or an out-of-range imported buffer index: generation/index rejection
returns no frame and clears `is_connected`. The driving and driver-monitoring
loops previously slept forever after that flag cleared. Reconnecting imported
buffers inside the old model loop would retain model/recurrent state across a
camera generation and potentially a layout change. The approved correction is
a terminal daemon error, allowing normal supervision to construct a new process
with fresh imports, catalog selection and model state. Ordinary timeouts continue;
requested shutdown remains graceful.

Supervisor source inspection confirms both catalog descriptors are native-bound
and restarted while their existing Onroad/DriverMonitoring predicates request
execution. Their `restart_if_crash=false` flags do not disable this path:
`Processes::ensure` first stops/reaps an exited child, then `start` creates a new
child for every wanted descriptor. `ManagedProcess::stop` clears the exited
handle; it preserves a live handle while asynchronous shutdown is pending.
No scheduling, inference outputs, camera pairing tolerance or validity limit is
changed. Verification results will be recorded after the focused tests execute.

#302 RED: the real Road-stream server replacement test failed with
`changed server generation must exit modeld` before the terminal-path change.
GREEN: full `openpilot-driving-modeld` and `openpilot-dmonitoringmodeld` package
suites passed 20 top-level tests. Real VisionIPC fixtures cover main, wide and
driver streams, changed server IDs, invalid buffer indices, ordinary timeouts,
graceful stop and fresh client imports with a changed allocation/layout. The
existing warmed-up snapshot reuse/non-aliasing regression still passes. The
new native supervision test verifies fresh PIDs for exited wanted model/driver
daemons with the catalog's ordinary crash policy, and no restart while unwanted.
Strict all-targets Clippy for both model packages and process supervision passed;
`git diff --check` passed. Disk remained above 388 GiB free.

The combined three-package test run exposed three existing parallel-sensitive
process-supervision unit fixtures: the two lock-transfer release checks returned
WouldBlock and the obsolete-generation fixture saw ETXTBSY. The complete
process-supervision suite passed all 28 tests with `--test-threads=1`; this is
recorded separately from the parallel failure. Their owner identified overlapping
forks inheriting deliberately non-CLOEXEC fixture descriptors and is repairing
the fixture synchronization. These host fixtures verify lifecycle contracts;
actual GPU model teardown/reload through normal device startup remains pending.
No original-source equivalence exemption was added for numerical inference,
publication, pairing or validity policies. The corrected generation-loss behavior
is intentionally different from the inherited infinite wait.

## Further verified host changes and target preparation

Hardwared's asynchronous Params writer now compares the current persisted
`NetworkMetered` value before writing an unchanged sample. The 2 Hz producer and
other Params writes retain their existing behavior. The pre-change regression
observed inode replacement for an unchanged value; after the change, 100 repeated
samples retained the same file while external changes, deletion, failed-write
retry and a subsequent value transition remained correct. All 10 hardwared
tests and strict Clippy passed. This removes redundant durable writes; no device
CPU or storage-lifetime measurement is claimed.

The Rust codec stager successfully built its pinned AArch64 libswscale with the
verified AGNOS 19.8-carrot-bt1 SDK. Separate C/C++ toolchain probes and the image's
OpenSSL binary ran under QEMU with that image's loader/libraries. Native and
original ELF reports for those four roots matched, with no ABI closure errors.
The unchanged released OpenCV native libraries and all installed headers were
restored against their exact manifest hashes, including both generated headers.
These are external-provider/toolchain evidence; source-matched project binaries,
complete package validation, normal device boot and recording/upload remain open.

## New-session continuation (2026-10-14)

The previous chat stopped during startup target linking. The same isolated
worktree and uncommitted source were recovered without discarding prior work.
The initial resumed AGNOS release build passed in 2m56s. Native static ELF
validation passed for 12 startup/helper roots and their 26 objects; each root
also passed loader `--list` through QEMU with the verified image's loader and
explicit package/AGNOS library directories. No project entry point or hardware
was executed by those checks. The installer transaction subsequently changed,
so this is intermediate build evidence and requires a fresh final rebuild.

The exact merged encoder revision `0de1a9402cc8ebfed6b85964805694ec95ccd400`
passed separate post-merge fast checks
[38547973318](https://github.com/bin9208/openpilot-rust/actions/runs/38547973318),
integration [38547973719](https://github.com/bin9208/openpilot-rust/actions/runs/38547973719)
and docs [38547973769](https://github.com/bin9208/openpilot-rust/actions/runs/38547973769).
Rust run [38547973330](https://github.com/bin9208/openpilot-rust/actions/runs/38547973330)
was still running at readback; these results do not cover the uncommitted #296 work.

New focused evidence:
- [USB persisted status writes #310](rust-model-usb-refresh-310.md): RED/GREEN,
  all 20 driving-modeld integration tests and strict Clippy passed.
- [Staged Git origin/upstream preservation](rust-runtime-lifecycle-resume-296.md):
  two RED/GREEN regressions, eight installer unit/45 CLI tests and Clippy passed.
- [Native FFmpeg builders](rust-native-ffmpeg-tools-296.md): native CLI/unit tests,
  actual AGNOS minimal static build and target header-only build passed.
- [Native media stager](rust-native-media-staging-296.md): six tests, original
  comparison, permission regression and Clippy passed.

Full developer-tool conversion, all-module audit closure, exact-source complete
package, required candidate CI and normal device startup/recording/upload remain
open. No device was connected or modified by this continuation checkpoint.

The continuation also replaced the generic USB companion and NumPy native-library
stagers. [USB staging](rust-usbgpu-stager-296.md) reproduced all 48 genuine
provider files (780,342,357 bytes) exactly, including the pinned model and
provider JSON; a same-size modified model was rejected. [Numerical staging](rust-native-torque-staging-296.md)
reproduced all 23 files from the source-locked NumPy wheel exactly and migrated
six CI consumers without changing numerical code or tolerances. Native stager
execution required no Python interpreter. The remaining Python model-export,
comparison, generation and fixture families remain in the tracked inventory.

Independent runtime review inspected the accumulated transaction/launcher,
generation ownership, model-frame lifetime/loss, logger/msgq reuse, sparse AE,
Params writes, sensor/microphone exits, SPI recovery and source-free/native
joystick changes without finding a concrete Critical/Important defect. This
source review supports candidate CI; it does not clear every module or prove
hardware operation. Separate tool reviews found two admission defects: matching
media bytes with wrong permissions, and a non-directory USB license ancestor
allowing partial publication. Both received failing regressions and focused fixes.
No review substituted for target or actual-device evidence.

Pre-CI checkpoint verification after the review fixes: all 124 native developer
unit/integration tests passed, as did strict all-target developer-tool Clippy,
workspace formatting, `git diff --check`, and the native documentation gate with
an explicit settings-preserving engineering exemption. CI policy passed 24 tests
and 367 subtests after requiring the replacement numerical-staging command.
Thirty tracked Python files are removed in this checkpoint only after native
replacement validation; 3,536 still remain on disk, including original-source
oracles and developer-tool families. This is a tested incremental candidate,
not complete source retirement or completion of issue #296.

## Exact-source CI follow-up (2026-10-14)

Candidate `0992e61fcadff2957c32705efc16ada9584557ed` passed both fast
checks and the user-doc gate, but Rust CI exposed four independent corrections:
GPS source-free build Clippy, the startup fixture's required installer entry
([#315](rust-runtime-startup-integration.md)), updater comparison of the approved
rollback-preserving lifecycle ([#314](rust-updated-staging-comparison-314.md)),
and OpenCV's download cache mutating immutable input
([#313](rust-native-opencv-cache-313.md)). Both architectures exposed the latter.

Follow-up validation passed full workspace strict all-target Clippy, workspace
formatting, 126 native developer-tool tests, 24 CI policy tests/367 subtests,
16 updater checker cases and focused Python lint. The actual updater comparison
retained 19 snapshots/322 ordered commands across five scenarios; direct and
normal-entry launcher fixtures each passed eight scenarios. The actual pinned
OpenCV CMake module reproduced the cache-path defect and corrected placement,
while source additions/edits still fail admission. All required CI and complete
AGNOS package builds must run again on the follow-up commit. No device result
is claimed by these local repairs.

## Complete AGNOS host candidate closure (2026-10-14)

Source `828efb0d2a8c9c9a2fa9e22f76f445832d6cc35d` passed all 11 bounded
release groups covering 89 project executables, with ION/native features and
the pinned AGNOS SDK. Compiler source metadata records that exact commit and
clean tree. The static musl installer replaces the ordinary GNU helper and
is tied to the build receipt. The native manager reports all 63 catalog
entries ready. Native ELF admission passed 92 roots/115 objects; all 92
AGNOS-loader dependency listings or static-installer usage checks passed.
The native sealer recorded 1,076 artifacts. These are host/cross-build, native
provider and package-composition results, without daemon/device execution.

Review found the missing OpenSSL provider for webjoystick (#316); its pinned
AGNOS executable/licenses are now staged, with five-object ELF closure, QEMU
version/RSA-4096 certificate creation and independent host certificate
verification. See [provider evidence](rust-runtime-openssl-316.md). The external
OpenSSL/AGNOS library dependency remains explicit.

The startup-services CI follow-up at `828efb0d` failed before its new pytest
cases because that job lacked pytest. Commit
`26ca85aad602bfe0e9661567e2bcfd69e7bc030a` adds the same pinned 9.0.2 dependency
used elsewhere. Its startup-services job in
[run 38556891074](https://github.com/bin9208/openpilot-rust/actions/runs/38556891074)
passed, including updater/process/AGNOS comparisons. Other required results
were still running at this checkpoint. The source-clean 828efb0d package
closure is intermediate evidence: subsequent CI/source corrections and the
[UBX recovery fix](rust-ublox-framing-317.md) require a fresh final exact-source
build and seal. No release/device handoff or complete Python retirement is
claimed by this checkpoint.

## Scoped module corrections after complete host build

The continuation's broader source audit produced four focused corrections:
[UBX framing #317](rust-ublox-framing-317.md),
[Athena retry pacing #318](rust-athena-log-retry-318.md),
[Wi-Fi worker ownership #319](rust-wifi-worker-lifecycle-319.md), and
[QCOM range admission #320](rust-qcom-diagnostic-bounds-320.md). Each has a
failing native reproduction, focused passing tests and strict package Clippy.
UBX preserves all 57 prior ordered source outputs and independently recovers
one NAV-PVT; its full decoder, daemon, Pigeon and ASan/UBSan serial gates pass.
Wi-Fi preserves 781 policy cases/2,322 frames plus both private-D-Bus gates.
QCOM preserves original valid mask bytes, 204 publications and six daemon
scenarios. Athena's original healthy forwarding/clock-boundary comparisons
pass in addition to failure/stop unit tests. The remaining QCOM delimiter-free
resource bound has no demonstrated general protocol maximum and stays open.

Independent reviews found no concrete Critical/Important defects in the UBX
or Wi-Fi corrections. Source review does not replace executed target checks.
The CI policy suite passes 24 tests/367 subtests; updater/UBX checker controls
pass 29 parametrized cases, focused Python lint and full-workspace strict
all-target Clippy pass, and the native docs gate passes with a concrete settings-preserving exemption. The final source
commit still needs its own target build, package seal, required exact-SHA CI
and normal device startup/recording/upload checks. The developer Python
inventory remains incomplete.

## Source-matched package and workspace CI follow-up

`7c8b2f0f05b1140d6d391275d3643ba3daa769b1` completed all 11 AGNOS build
groups and 89 project executables. Manager readiness remained 63/63; native
ELF closure (92 roots/115 objects), all 92 loader/static-installer checks and
sealing of 1,076 artifacts passed. The native package builder validated and
published its local archive atomically: 1,155,600,591 compressed bytes,
1,708,024,389 unpacked bytes, SHA256
`8d9c9a9f330ce8b7401b0a3ef1e49231036d5206e9331934eeb03b6334cb988d`.
This is premerge host evidence, not a release or device result.

[Integration 38560531570](https://github.com/bin9208/openpilot-rust/actions/runs/38560531570)
passed all 16 jobs; fast checks and mapped docs also passed. The same-source
[Rust run 38560531619](https://github.com/bin9208/openpilot-rust/actions/runs/38560531619)
found a workspace-only model catalog serialization failure: unified
`serde_json/preserve_order` changes index key order and generation identity.
[#323](https://github.com/bin9208/openpilot-rust/issues/323) tracks the repair;
original graph field ordering and sorted catalog bytes are separate contracts.
The PR remains unmerged while required checks fail or run. A successful future
merge still requires separate exact-merge-SHA CI and source-matched packaging.

The [#323 catalog correction](rust-model-catalog-order-323.md) reproduced the
existing byte-fixture failure with `preserve_order`, then passed all 43 model
runtime tests and strict Clippy both with and without that feature. The
[#325 scalar-access optimization](rust-locationd-scalar-access-325.md) removes
unused locationd snapshots while retaining current post-rewind state. Its native
12-test suite, original 1,073-step comparison and ASan/UBSan gates passed; full
pre/post native trace bytes agree. These follow-ups require a new exact source
commit and its own CI/package evidence. They do not change settings, validity
thresholds, model math or device-test authorization.

`430ec72bf4fd5bc53dafb0e262b815e76c38ff37` subsequently passed the complete
11-group/89-executable AGNOS build, static-musl installer binding, 92-root /
115-object ELF closure, all 92 loader/usage checks and the 1,076-artifact seal.
Native release construction and archive verification passed: 1,155,614,656
compressed bytes, 1,708,024,293 unpacked bytes, archive SHA256
`66153c3a29b9cc46e9ee3d16b1ab78032221249f715c8f6e75dd7a55abc6debb`,
manifest SHA256 `5242c8e4a20145aa8febcec6b5260c173c9b4540946d98bd2cd14c9189543948`.
These source-matched local artifacts remain premerge evidence.

[Run 38566168156](https://github.com/bin9208/openpilot-rust/actions/runs/38566168156)
then exposed a separate upload EINTR fixture race: all 14 upload timeout cases
passed, but a two-read `/proc/.../children` predicate observed digits followed
by an empty value and failed before its protocol comparison. This is tracked
in [#327](https://github.com/bin9208/openpilot-rust/issues/327). The correction
must preserve original/native request, signal and deadline assertions and make
failed-launch cleanup observable. It is a checker correction, not evidence of
a runtime upload failure or completed Python-tool conversion. Subsequent
checker-only commits use the complete 430ec72b build as prior evidence; the
final merged source still requires its own package identity and post-merge CI.
