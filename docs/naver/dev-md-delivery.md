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
