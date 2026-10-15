# dev-md installable candidate (#338)

The source-only handoff was incomplete. The user's 2026-10-16 correction requires
the exact-source AGNOS runtime package and public installer assets so the user
can perform the real-device test. No device is accessed by this task.

## Delivery boundary

- Keep all source changes on dev-md; do not advance dev or main.
- Build the selected Rust executables against AGNOS 19.8-carrot-bt1. Generic
  Linux ARM CI binaries are not deployment artifacts.
- Reuse external companions only with verified hashes and unchanged source
  compatibility. Rebuild Panda firmware for the latest Tesla safety header.
- Include the new MD v3 assets, selected daemon features and manager-required
  source assets. Never relabel old model or runtime binaries.
- Use the existing native manager inventory, seal, package and publication
  commands. Check clean source identity, ELF dependencies, installer admission,
  exact-SHA CI and downloaded release hashes before reporting readiness.
- Publish a prerelease named `runtime-<full source SHA>` in
  `bin9208/openpilot-rust`, without making it the latest stable release. The
  existing `ensure_runtime.sh` resolves that exact tag from the checked-out SHA.

The publisher now accepts explicit `--branch dev-md`. The default remains dev.
Both branches retain the same required-check policy; source SHA, push event,
repository, workflow, job and required-check identities are checked before and
after upload. dev-md cannot borrow a dev run, and unsupported branch names are
rejected. Ten publication tests, including the dev-md success and rejection
cases, pass. Reading dev's required-check policy does not modify dev.

## CI compatibility repairs

The first source candidate exposed stale comparison inputs and missing CI
dependencies. Repairs keep runtime behavior and strict comparisons intact:

- Fetch the two exact pinned upstream commits for source-reference loaders.
- Bind changed planner/angle/Xiaoge/stopping source helpers to those pins rather
  than retained baseline Python implementations.
- Supply the staged numerical provider path to the new RadarCAN tests.
- Keep optional native OpenCV activation explicit, matching other native crates;
  the packaged signal daemon still selects its required native features.
- Align Panda's isolated child firmware fixture, DM timing schema and manager
  stop-trace assertions with the changed producer contracts.
- Account for the 22 remaining V4L and six runtime cases after source YouTube removal; each case
  retains its source/native comparison.
- Preserve the inherited unknown-road HUD display, align stale HUD test setup,
  and bind startup graphics references to the selected source.
- Select the final overlay owner for existing root/augmented scene comparisons;
  compact transitions (100 frames) and augmented (32 frames) retain exact pixels
  and state. Install the missing host GLES development library for renderer CI.
- Apply Rust formatting and store the unpublished Wiki patch with zero context
  (`git apply --unidiff-zero`) so patch context spaces do not fail whitespace CI.

Focused local evidence includes the publication tests, two fresh Hyundai ACC
tests, three Panda composition cases, 436 Xiaoge policy cases, actual Xiaoge
OpenCV reference generation, 36 HUD tests, and 384 exact planner reference
frames. These results do not replace the final exact-SHA CI or package receipt.
The exact final SHA, Actions links, release URL and download verification are
recorded in issue #338 after publication, without changing the sealed source.

Repository-wide tool retirement remains excluded. Hardware startup, GPU
inference and driving acceptance remain user-owned and are not claimed from
host/QEMU packaging checks.

## Fresh-install CAN assets

Card previously selected `ROOT/opendbc_repo/opendbc/dbc` even in an installed
runtime. The repository tracks only seven generated DBC files, while the sealed
`rust/bin/radarcan-dbc` companion supplies the complete 103-file set. A fresh
installation must not depend on ignored generator output left in a developer's
source checkout.

Card now accepts an optional explicit `--dbc` directory. Standalone use retains
the existing source-directory default; the manager supplies and requires the
sealed companion. Three parser checks and the selected binding check pass on
host and actual ARM/QEMU. The final package rebuild includes both the changed
Card and manager bindings before sealing.

## CI reference reconciliation and Xiaoge telemetry

The Hyundai fixture stages now install the existing pinned reference package before
CAN imports, so state, controller and packet fixtures share the final stopping and
blinker behavior. All eight existing stages generate successfully; the fresh host
controller test and postcheck match 35,520 actuator schemas and 91,271 CAN messages.
The five fixture-caller checks pass. Production Hyundai behavior is unchanged.

The original ARS510 private jerk-state fixture was captured with NumPy 2.5.3's
OpenBLAS SkylakeX kernel. Another CPU kernel can produce a roughly 4e-14 difference
in private filtered jerk despite identical library bytes. CI now replays the same
2,697 checked actions through the pinned original receiver on its actual CPU.
The reference refresh verifies shared source hashes and equality of source NumPy
and sealed native numerical-library hashes, and records the selected CPU kernel.
No native output constructs expectations; public and private comparisons retain
exact floating-point bits. Local reconstruction reproduces all 2,707 original
rows and 1,561,339 numeric comparisons exactly; both fresh native receiver tests
pass against that independent reference. Production arithmetic is unchanged.

The car-specific coverage check now identifies a multiline assignment's executed
RHS line rather than its later target line. The unchanged corpus passes 29,787
steps, all 162 outcomes of 81 source conditions, 51 event branches and eleven
invalid-input checks; no condition was exempted.

The corrected Xiaoge live reference exposed an actual omitted upstream telemetry
change. Native packets now include up to four lane probabilities/stds/lines, two
road-edge stds/lines and 33 coordinates per axis, plus the live controlsState
packet and subscription. The pinned SelfdriveState schema lacks vCruise, so its
source fallback remains zero. Existing live inputs supply nonempty geometry and
controls data. Nineteen source/native phases pass with actual ONNX, VisionIPC,
TCP, Cereal, snapshots and reconnect/shutdown. The probe validates per-side
publication timestamps and validSides; it removes elapsed timestamps and checks
round-robin display-side admissibility before comparing stable phase results.
When the camera is unavailable, the probe asserts the retained scheduler cursor
belongs to eligibleSides before canonicalizing that asynchronous cursor; reasons
and all per-side fields remain compared. Freshness waits require every eligible
side to be valid; the Params refresh
explicitly waits for the requested inference side, lane threshold 0.73 and BSD interval 0.6 seconds before
subsequent phases. The final receipt uses the freshly rebuilt telemetry binary.
Xiaoge native all-target strict Clippy passes. These are host checks; the final
AGNOS package rebuild and user device acceptance remain separate.

Receipts: `.analysis/scratch/2026-10-16-dev-md-scope-excluded/` contains
`card-controller-final-{native,postcheck}.log`, `card-fixture-caller-tests.log`,
`ars510-source-reconstruction-check.log`, `ars510-local-reference-native.log`,
`car-specific-coverage-final/manifest.json`, `xiaoge-live-final8/result.json` and
`xiaoge-telemetry-clippy.log`.


## Radar reference cohort reconciliation after CI 6007

The radard geometry CI reference imported the retained baseline predictor while
Rust implements the pinned terminal-path tangent. The existing hash-verified
radar source snapshot now supplies the six existing recorders, their unchanged
pinned tests, replay metadata helper, and the source IPC/lifecycle daemon.
Unchanged modules that capture predictor imports are reloaded in dependency
order; the package exports refer to that same cohort. Source receipts retain the
pin and content hashes. No native arithmetic or comparison tolerance changed.

Host comparisons pass exactly: 128 geometry cases; 23,126 selection, 14,329
primary, 10,742 trajectory, 5,524 cutout-predictor and 5,579 controller actions.
The existing six actual IPC scenarios compare all 240 publications exactly;
eight source/native lifecycle exits and six preconditioning tests pass.
Receipts are under `.analysis/scratch/2026-10-16-dev-md-scope-excluded/`:
`radard-cohort-final-geometry/receipt.json`, `radard-cohort4-*/receipt.json`,
`radard-cohort-final-ipc/receipt.json`, `radard-cohort-lifecycle/receipt.json`
and `radard-cohort-preconditioning.log`. These reference-only repairs require
CI confirmation on its host and ARM runners; they do not establish device use.


The RadarCAN constructor reference had the same retained Toyota-module mismatch
and omitted the production CarParams brand. The source fixture now selects the
pinned Toyota modules before interface discovery, supplies the real candidate
brand, and records the final ARS510 receiver-owned constructor state. All 558
existing constructor cases pass with exact native/source state and parameter
reads; receipt: `radarcan-constructor-final2/result.json` in the same scratch
root. No production code or input case count changed.

## Remaining existing-gate repairs after 6007

- The hardware source job now fetches the exact source pin. With that object
  available, all 15 existing hardware worker cases and continuous hardwared IPC
  transitions pass locally, including Params, logs, stats and SIGTERM.
- The planner main fixture executes the verified final signal-mode constructor
  lookup. All 712 Params rows and 1,080 publications match the ARM artifact.
- Selfdrived IPC no longer waits for a driverCameraState reader removed by the
  pinned DM2 camera policy. Two startup/restart runs, two nonconflated bursts and
  two Params refreshes pass through real IPC (206 observed messages).
- The DM2 boolean expression is simplified for strict Clippy; all eight Boolean
  combinations retain the same result.
- The Jetlink ASan fixture step runs serially so instrumented socket fixtures do
  not compete for the unchanged 50 ms production RPC deadline. The isolated host
  test and all 35 library tests under the same nightly ASan/build-std flags pass
  serially; production deadline and protocol checks remain unchanged.
- Xiaoge configuration-transition fixtures wait for both sides' exact new source
  confidence values before recording. This removes stale right-side snapshots;
  the existing 19 real source/native phases pass with unchanged comparisons.

These repairs do not authorize a new broad runtime or development-tool port.
Final publication still requires the exact clean source commit and its gate results.


Card's later Tesla comparison likewise imported baseline EPS-inhibit behavior.
The Tesla oracle now selects the complete latest `8e7eec04` package before CAN
imports capture checksum helpers. All 137 existing cases / 4,960 frames match
fresh native output exactly; the corrected source fields also match the actual
CI 6007 native capture exactly. Receipt: `card-final-tesla/result.json` in the
same scratch root. The five existing card CI caller tests pass. This changes
validation source selection only; EPS/controller behavior remains unchanged.

The following existing Toyota comparison also selects its pinned package before
CAN helper imports and records those actual source hashes. It passes 1,348 cases
and 24,008 runtime frames, including 74 before-update failures and 13 numeric
failure cases, with its existing comparison rules unchanged. Receipt:
`card-final-toyota/result.json` in the same scratch root. No further native
runtime edits were needed for either brand.


## Remaining Card/RadarCAN CI chain and Card owner deltas

Running the remaining existing CI chain exposed stale Card IPC references: the
source owner and admission fixture still had nine subscriptions, versus the
final thirteen. The source owner now uses the pinned Card/cruise/lateral-readiness
and Toyota PCM helpers, with final Hyundai/Toyota/latest Tesla packages selected
before CAN imports. The fixture publishes and validates all thirteen topics with
the same strict immutable-identity admission checks; no comparison was relaxed.

That corrected reference exposed actual missing Card owner hunks. Native Card
now records OEM/ONNX split fields before merging blindspots, marks the split
valid, and publishes the source metadata on changes or the one-second heartbeat
before carState. The final ImpactDashcamReboot application boundary clears
engagement and actuators, clears resume/override, and cancels active stock
cruise. Its previous-enabled state retains the inhibited command, as the source
does. Native runtime diagnostics now report truthful `can_backend=rust` at the
existing source diagnostic boundary; the source oracle asserts its Python
backend context separately from the numeric metrics. Existing subscriptions,
lateral startup latch, speed-camera context and consumed-gas hooks cover the
other direct Card.py changed hunks.

Five native core tests and strict native Card all-target Clippy pass. The same
seven core cases / 5,928 steps compare exactly against the full pinned owner
methods, including 67 ordered metadata publications and reboot transitions
inside the existing sixteen startup steps. All existing remaining Card brand
references pass, as do 182 factory constructors and the 114-case CAN catalog.
The 10,264 checksum cases retain their declared inherited vector failure, and
the PSA checker retains its declared source-blocked boundary; neither result
claims new platform support. The 39 admission/phase/caller tests pass.

All nineteen existing pumped IPC scenarios (eighteen candidates) pass eighty
exact measured frames each, with shutdown and asynchronous Params drain. The
first two Body scenarios were additionally rerun after the final guard rebuild;
all other scenarios started after that build. Final host ELF SHA-256:
`b4c36decbbe420c101bcb9538c99b60e0efb5e82d000746f7222e5d1114eca2d`.
The remaining RadarCAN chain passes 23 normal, five joined and fifteen lifecycle
scenarios without further changes.

Receipts in `.analysis/scratch/2026-10-16-dev-md-scope-excluded/`:
`card-attribution-source-core-final/result.json`,
`card-attribution-core-tests-final.log`, `card-attribution-clippy-final.log`,
`card-attribution-ipc/pumped/result.json`, `card-attribution-final-body/result.json`,
`card-remaining-*/result.json`, `card-remaining-admission-tests.log`, and
`radarcan-remaining-ipc/result.json`. Card's production binary changed and must
be rebuilt in the final AGNOS candidate; these host results do not replace the
final source-matched ARM package or user device acceptance.

The remaining planner daemon fixture now shares the same pinned owner modules:
four real IPC runs (121 publications each), 13 source/native lifecycle cases and
360 owner frames compare exactly. The final selfdrived failure checker passes
all nine cases, treating malformed retired DisableDM bytes as ignored while
retaining exact bounded output counts and every remaining fatal-input check.

Toyota/Tesla reference extraction now explicitly uses the repository root when
invoked from the workspace's rust directory, and rejects an empty source package.
The exact previously failing workspace invocation regenerates all 2,697 ARS510
actions successfully with verified source/provider hashes.

## Final workspace references and installation proof

The existing workspace gate also contained stale fixtures for the new empty
`external_compute_label`, Card's sealed DBC requirement, and the firmware content
identity after the latest Tesla safety change. Both cluster default fixtures now
include and explicitly check the empty label. The installed Card fixture checks
the DBC directory before its existing numerical-provider requirement. Original
`panda/firmware_version.py` independently produces `DEV-d48f6455ef1d-DEBUG`;
the version/header fixtures and their hashes now record that identity. No build
tool implementation changed. The six staged firmware files already match this
identity, and all 252 recorded firmware source inputs remain unchanged.

Workspace feature unification enables JSON insertion order. RadarCAN comparisons
now compare object key sets, retaining exact numeric bits and all array ordering,
including explicitly recorded source-order arrays. This corrects comparison of
equivalent JSON objects; no runtime output or expected numerical value changes.

One existing workspace `--no-fail-fast` run collected all remaining failures.
All 19 failed targets subsequently passed: three stale-fixture targets, ten
RadarCAN targets with explicit `serde_json/preserve_order`, and six targets whose
local SDK libraries or FFmpeg path were missing. The latter were rerun using the
original workspace test executables and the required local library/encoder paths.
No second full workspace run or expanded test corpus was added. Receipts are
`workspace-final-no-fail-fast.log`, `workspace-failed-targets-recheck.log`, and
`workspace-environment-targets-recheck.log` under the delivery scratch directory.
Formatting, whitespace and the mapped user-docs check also pass.

The first complete package for `375a0ac960c27498a83335679586768b92f1598b`
passed the native seal (1,082 artifacts), actual static ARM offline installation
into an independent checkout, and the installed ARM launcher's package check
under QEMU with the AGNOS 19.8-carrot-bt1 SDK. No hardware child was started.
The 1,178,558,449-byte archive has SHA-256
`9a8715bc1837082c6838a018b8f0861a96828cdc9d44599c9404e649e4b57096`.
Receipt: `.analysis/scratch/2026-10-16-dev-md-package/install-proof-375a.json`.
Publication remains held until the reference repairs have their own exact-source
CI results and rebuilt package; this earlier package is not relabelled.

The startup CI's owned package copied seventeen debug providers. Its unchanged
20-second lifecycle deadline also included authenticating 602,543,128 bytes of
mostly debug data, repeatedly expiring before model/manager startup on CI.
Captured CI executables with only debug sections removed retain identical
`.text` hashes and reduce that package to 38,150,040 bytes. Package verification
on the same host changes from 13.0667 to 0.8144 seconds, and all fourteen original
bootstrap scenarios pass with the unchanged checker and deadline. The startup
job now sets `CARGO_PROFILE_DEV_DEBUG=0`; runtime code, package authentication,
assertions and all timeouts remain unchanged. Evidence is under
`startup375a-no-debug/` and `startup375a-no-debug-bootstrap.log` in the delivery
scratch directory. This CI build-profile repair does not convert any tool.

The recovery fixture exposed an accidentally tracked absolute `node_modules`
symlink to the developer's private dependency cache. It is removed from Git while
the local dependency directory is preserved. The web ignore rule now covers
both directories and symlinks; the minimal recovery package excludes build-only
dependencies. All remaining tracked symlinks are relative. The eight existing
recovery checks pass, including standalone HTTP, terminal/lifecycle, support,
shutdown, 24 action scenarios, upload comparisons and fourteen CWP scenarios.
Receipts: `recovery-http-existing.log` and `recovery-existing-chain.log` in the
delivery scratch directory.

After the e9e6 workspace recovery-copy failure, all 36 existing downstream
functional checks were completed using the current host executables. Evidence
is indexed by checker name in
`.analysis/scratch/2026-10-16-dev-md-delivery/workspace-tail-checks/*.log`.
This covers USB transport/ASM, retention (including an actual private low-space
filesystem), Jetlink policy/RPC, service/message state, torque, calibration,
desire/commands, monitoring, diagnostics, model inputs/timing, procLog, Params,
logging producers/collectors and continuous daemon IPC. Monitoring compared
209,144 source states and complete packets exactly; dmonitoringd compared 329
complete packets. The four existing msgq ASan/UBSan harnesses rebuilt their
native boundary and exited successfully (`msgq-sanitizers.log`). Original Python
IPC bindings were reused for the daemon checks; the low-space check used its
supported unprivileged mount namespace. The consumer probe was local host IPC.

The service checker still classified `customReservedRawData1` as absent despite
its final schema definition. Removing that stale exclusion restores the existing
88-service catalog check; generated definitions and native wire/order assertions
are unchanged (`services-final.log`). The model owner's final-pinned USB status
fixture repair separately passes all 192 existing cases and 24 native controls.
That fixture now reads the final upstream Git pin and explicitly binds its
synthetic model to a custom manifest. Previous-only state follows the source's
current selection rules. Authentic default MD v3 admission and its runtime guards
remain unchanged; no synthetic model is represented as the production model.
Receipt: `.analysis/scratch/2026-10-16-dev-md-package/usbgpu-status-final-pin.json`.
No downstream functional check remains unrun. On `7b2b7e4144a03b73a70d86a7ba30ef54251a10de`,
the complete workspace job, release build and CPU sample passed in
[Rust CI run 38807666068](https://github.com/bin9208/openpilot-rust/actions/runs/38807666068).
Forty-five jobs passed; the separate static ARM build exposed a MUSL ABI
initialization error. These results do not establish device startup or driving
acceptance.

## Static ARM libc compatibility

Three scheduling boundaries now initialize the complete `libc::sched_param`
before setting the original priority, including MUSL's additional fields.
Recovery, signal observation and encoder scheduling policies, affinity and nice
values remain unchanged. Two native encoder ioctl calls infer the libc request
type, preserving the same Linux UAPI low 32 bits for GNU and MUSL signatures.
No development tool conversion is included.

The complete static AArch64 MUSL workspace release build passed in 22m29s with
the authentic FFmpeg 6.1.1 MUSL provider. The native encoder feature separately
linked statically with its real FFmpeg/swscale provider and original AArch64
C-ABI libyuv archive; ELF inspection found neither an interpreter nor dynamic
dependencies. The existing QEMU synthetic-proc probe passed three frames.
Six recovery tests and strict all-target Clippy for the three affected crates,
including the native encoder feature, passed on HOST and AGNOS GNU targets.
Receipts are `musl-workspace-fixed.log`, `musl-native-encoder-v2.log`,
`musl-probe-fixed.json`, `musl-fix-host-recovery.log`,
`musl-fix-host-clippy.log` and `musl-fix-gnu-clippy.log` under
`.analysis/scratch/2026-10-16-dev-md-package/`.
The resulting clean commit requires its own CI and rebuilt AGNOS package before
publication; the earlier verified `7b2b` package is not relabelled.
