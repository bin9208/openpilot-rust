# Native selfdrived (#168)

The complete component is tracked in
[#168](https://github.com/bin9208/openpilot-rust/issues/168), under full-runtime
#1. Its preimplementation scope is in
[the runtime contract](../rust-port/selfdrived-contract.md).

Current status: the native component has reviewed host evidence, ARM/QEMU
controller/runtime evidence and offline dynamic linkage against the pinned AGNOS
image. Complete manager startup/log upload and user device acceptance remain
open. The earlier sections below record the implementation stages.

The first verified stage implements the engagement state machine. All five
source states, ten event categories, disable/pre-enable/override precedence,
300-cycle soft-disable expiry and alert-category ordering are retained. Three
focused sequence regressions pass. An unchanged-source method comparison
passes 51,023 steps, including all 1,024 category combinations from each state
at six timer boundaries plus continuous/randomized sequences. The checker
reads DT_CTRL from the original realtime source. State, timer, enabled/active
flags and ordered alert categories compare exactly.

Local evidence is under the ignored `2026-10-01-rust-selfdrived` workspace:
`state-red.log` records the initial missing module, and `state-oracle-first`
retains inputs, independent source/native outputs and source/binary hashes.
Strict package Clippy, formatting, Python Ruff and whitespace checks pass.

Alert catalog/callbacks, event generation, car-specific policy, calibrated pose
checks, native continuous IPC/Params and full startup integration are still in
progress. This stage does not mark selfdrived ported, authorize production
selection or establish vehicle/performance acceptance. No device was accessed.
## Alert storage and event dispatch

The native alert manager preserves insertion-order ties, priority/start-frame
selection, repeated-alert minimum duration, expiration and category clearing.
The original class bodies match all fields across 25,000 randomized steps.
Evidence is retained in `2026-10-01-rust-selfdrived/alert-manager-first`.

The complete 125-event catalog for both tici and mici is generated from the
unchanged source classes, static definitions and full cereal enums. Generated
data retains exact source/schema SHA-256 values. All 24 used callback kinds
are typed; callback bodies are a subsequent stage, not a runtime fallback.
The native event container matches 4,872 original-source steps covering sorted
duplicates, static persistence, counters, creation delays, category order,
callback dispatch, translation-copy behavior and hardware-specific definitions.
The callback boundary uses controlled alerts in this dispatch comparison;
it does not establish callback-body parity. See `events-final/manifest.json`.

Nine Rust behavior tests, strict all-target Clippy, formatting and Python Ruff
pass. Unknown schema enum values, missing catalog entries and callback failures
return explicit errors. No Python source executes in the native crate.
Continuous selfdrived, health gates and whole-runtime integration remain open.

## Dynamic alert callbacks

All 24 callback kinds used by the source catalog now have native bodies. The
native code reuses the locale parser and original translation data, preserving
hardware-specific alert ordering, personality spelling, Params read order,
camera/process ordering, display rounding and ordered `max` behavior for NaN.
The source oracle executes the original callback and translation class bodies,
with constants and Hyundai flags read from their original definitions.

`callbacks-wire-final/manifest.json` records 15,852 cases across both hardware
layouts and 12 languages. The 15,648 alert-producing cases match all fields and
ordered parameter reads. Another 180 NaN/infinity cases reproduce the original
integer-rounding failure as an explicit native error. The original exceptions
are retained separately, alongside source/schema/translation and binary hashes.
Finite cases include adjacent binary64 values around rounding boundaries and
large magnitudes. The earlier `callbacks-boundaries` run completed comparisons
but failed writing provenance due to an incorrect schema path; it is retained.

The remaining 24 cases make `NNFFModelName` absent. The original callback
returns a null second text, and actual cereal wire assignment subsequently
raises a type mismatch. Native typed alerts reject that missing text during
callback resolution. Both reject the input, but the failure phase differs;
these cases are not claimed as exact callback-return or full-daemon equivalence.
Continuous-loop integration must retain the failure and its effect ordering.

All-target strict Clippy, formatting, Ruff and diff checks pass. The callback
comparison currently supplies the Params interface; the real Params adapter,
event generation, car-specific logic, calibrated pose and continuous daemon
are subsequent stages. This does not mark selfdrived ported.

## Physical alert Params adapter

The callback interface now has a native file-backed adapter. STRING reads reuse
the original-compatible UTF-8/logging boundary, integer reads reuse the verified
`std::stoi` semantics, and boolean reads match exact byte `1`. File read errors
produce the source getter's empty value; unknown keys and fatal integer casts
remain typed errors. No fatal cast is defaulted to zero.

`params-first/report.json` records 47 comparisons against the actual original
Cython Params binding, using synthetic files and private logging sockets.
Coverage includes the four callback keys, missing/empty/NUL/Unicode/malformed
UTF-8 values, permission/directory reads, signed limits and trailing integer
text, unknown keys, and seven invalid/out-of-range integer reads. Those seven
source child processes actually terminate with SIGABRT; the native adapter
returns its explicit fatal integer error for propagation by the daemon.
Warnings and their transported messages match the original. Strict all-target
Clippy, formatting, Ruff and diff checks pass. Continuous-loop effect ordering
and the remaining selfdrived policy are still in progress.

## Pose and excessive-actuation helpers

The native PoseCalibrator reuses the existing rotation, composition, vector and
covariance operations. It preserves calibration state, all four measurements,
and unknown orientation standard deviations. ExcessiveActuationCheck retains
the original longitudinal/lateral thresholds, strict 25/100-cycle boundaries,
steering override reset, pose-validity comparison and longitudinal precedence.
Infinite roll fails at the source sine operation's boundary; NaN remains a
numeric value with the original false-comparison behavior. Camera packet order
and all DisableDM/simulation/wide combinations are preserved.

`helpers-final/report.json` records 10,241 original-source steps: 4,015 pose
transformation cases, 6,198 actuation sequence steps and 28 camera selections.
Floating pose values match within 2e-12 absolute/relative error; calibration
flags, counters, camera order, actuation choices and error results match exactly.
Tests include adjacent representable values around thresholds, reset/override
sequences, rotation boundaries, negative standard deviations and nonfinite roll.
Source files and the executed binary are hashed in the receipt.

Focused strict Clippy and Ruff pass. The first Clippy run rejected the explicit
two-sided float comparison; the comparison is now expressed as named
acceleration/deceleration predicates, retaining NaN semantics without disabling
the lint. Generic native message decoding and the continuous selfdrived loop
remain subsequent work. No validity or safety threshold has been relaxed.

## Cut-in audio event helper

`cutin::promoted` and `cutin::Tracker` port the unchanged alert candidate matching
and repeat suppression helper. Selection requires the existing nonnegative
leadTwo identity and the original three 0.1 thresholds; repeated physical
objects retain the original same-ID/re-ID distance, lateral and velocity limits.
Disabled updates and explicit resets clear previous candidates, and candidate
order and duplicates remain observable. Radar detection and lead selection are
unchanged; this helper only decides the existing selfdrived audio event.

`cutin-final/report.json` records 5,786 exact source/native steps covering both
selection and complete retained candidate state. Cases include boundary-adjacent
binary64 values, negative/zero/large IDs, duplicates, enable/reset sequences,
5,000 seeded mixed frames and NaN/infinite/signed-zero inputs. Input/output bit
patterns preserve these values through the fixture protocol. Source SHA256 is
`ed39e0ad32462ee7ff2ef605303a06282406fe81d21d458d4495ecfc27a521e4`;
native example SHA256 is
`e16e0f55bb130f0a13a5c0ee7b2496efa49071f04ece078180573fe3d9963589`.
Bounded build, strict library/example Clippy, Ruff and diff checks passed.

## Complete continuous host component (2026-10-02)

The interrupted controller/runtime changes are now completed and independently
verified. `rust/crates/selfdrived/src/controller/` owns source sampling, event
generation, health gates, pose/actuation integration, alert resolution and wire
publication. `runtime/` owns physical Params, sockets, Ratekeeper, settings
refresh and cleanup. The native executable has no runtime Python dependency;
captured `/proc` executable identities and maps verify the process used in QA.
The original msgq/VisionIPC CXX boundary and libzmq remain native dependencies.

Null second alert text now survives every source phase up to selected-alert
wire assignment; both implementations fail there. Ordered effects, manager
state and earlier publication behavior compare exactly. Process-failure logs
retain source set-repr text instead of a JSON array, including quotes, controls
and Unicode. Only unspecified set order is normalized in the oracle.

Evidence is retained inside this worktree at
`.omo/evidence/selfdrived-resume-20261002/`. `LEDGER.md` gives the exact scenario,
invocation, binary observable and captured paths for each completion criterion.
`receipt.json` binds authoritative and owned source hashes, executed ELF hashes,
artifact sizes/hashes, the preserved dirty base and limits. The `proof/` directory
retains exact executable hardlinks without additional binary disk allocation.

| Criterion | Fresh observed result | Evidence directory/file |
| --- | --- | --- |
| All five engagement states/category/timer boundaries | 51,023 exact steps, including 30,720 exhaustive single steps | `state-verified/result.json` |
| Alert tie/expiry/replacement and event storage/translation/callback dispatch | 25,000 manager steps and 4,872 event steps exactly match source | `alert-manager-verified/manifest.json`, `events-verified/manifest.json` |
| All 24 callback kinds and source failure phase | 15,876 cases across tici/mici and 12 languages; 204 original exceptions retained | `callbacks-verified/manifest.json`, `source-exceptions.json` |
| Full controller, Params effects, health/state and actual wire | 295 requests / 9,436 steps exactly match; source personality KeyError and cereal KjException retained | `controller-verified/manifest.json`, `source-coverage.json` |
| Pose, actuation and camera policy | 10,241 source steps; discrete results exact, pose tolerance 2e-12 | `helpers-verified/report.json` |
| Ratekeeper lagging and all moving-average state | 20,000 exact steps | `ratekeeper-verified/manifest.json` |
| Owned continuous start/engagement/nonconflated inputs/settings/stop/restart | Two native starts, two ten-message queue bursts, two Params refreshes, both publications and SIGINT joins | `ipc-verified/manifest.json`, message captures and `native-processes.json` |
| Startup/input/runtime rejection and bounded exit | Nine real-process scenarios: waiting SIGTERM, malformed CP, fatal integer SIGABRT, wrong union, malformed state, unknown gear/personality, null alert text, 20-frame exit | `failures-verified/manifest.json`, per-scenario captures and bounded queue bytes |
| Package and messaging seam regressions | Nine selfdrived tests and seven messaging tests pass; no failed/ignored tests in their full runs | `test.log`, `messaging-test.log` |
| Build/lint/format/CLI | Selected native executable/examples build; all-target strict Clippy, Rustfmt, Ruff and help/bad-argument exits pass | `build-final.log`, `clippy.log`, `validation.json`, CLI captures |

The complete car-specific policy is unchanged from the separately recorded
29,787-step proof. Its exact retained ELF and all owned/authoritative source
hashes were rechecked before reuse; `car-policy-receipt-check.json` records that
audit. No unverified old report is used as a current-binary lifecycle result.

Early continuation failures remain captured: wrong Python module search path,
process-log array/text mismatch, the Ratekeeper oracle's aliased mutable buffer,
empty legitimate onroad event payload, and initial subscriber handshake loss.
The corrected timing oracle copies each buffer snapshot. The bounded-frame
check decodes the owned shared queue after exit and proves exactly 20 valid
state messages, including frames initially missed by the connecting subscriber.

Build preflights preserved the required free-space floor and the coordinated
statsd cache budget; incremental compilation was disabled. The existing large
callback catalog dispatcher is retained as one exhaustive source mapping;
new controller/runtime production modules remain below 200 pure lines.
No production selection changed. At that host checkpoint, ARM/AGNOS execution, actual FIFO/core
placement, complete normal startup/log upload and user vehicle comparison remain
#1 integration/acceptance work. No device, vehicle, physical CAN or NAS was used.

## ARM continuation (2026-10-03)

Before this continuation, all 80 files in the independent host review matched
their recorded hashes. The native production code remains unchanged; only the
IPC checker gains an explicit underlying ELF argument for emulated execution,
and the manager catalog records the available native candidate. Source process
selection and the original onroad predicate remain unchanged.

The retained ARM executable is
`917bf13e83977ed9026c0585c76dd42194305482e6cf24d1ca29873a29c51b1c`.
The bounded package build and two example builds passed with incremental
compilation disabled and a checked disk reserve. ARM controller replay exactly
matches all 295 retained host output records, representing 9,436 steps; the
Ratekeeper replay matches 20,000 records. The paired original-source/host input,
output and result hashes were verified before reuse; the original Python source
was not rerun for these architecture comparisons.

Actual ARM main execution under QEMU passes two starts/restarts, both
nonconflated ten-input bursts, both Params refreshes and SIGINT cleanup. Real
subscribers received 47 selfdriveState and four onroadEvents messages. Captured
maps and command lines identify the retained ARM ELF and QEMU, with no libpython.
All nine real-process rejection/termination scenarios also pass, including
SIGABRT on the source fatal integer conversion and exactly 20 queued messages
for the bounded run. Message counts depend on emulation scheduling and are not
a target performance measurement.

Nine selfdrived and seven messaging tests pass on ARM. The first Cargo test
runner could not launch the two messaging test children: a retained execve trace
shows ENOEXEC because nested ARM execution has no host binfmt handler. Those two
unchanged test bodies were then launched through QEMU with the same isolated
namespace and child environment, preserving every assertion. An initial direct
runner used an invalid namespace prefix and failed the existing isolation guard;
correcting that fixture prefix passed. The earlier five passing pure messaging
tests were hash-checked and reused.

The native catalog probe also passes all four onroad/offroad and car/not-car
combinations, with outcomes false, false, true, true. Its initial host build
encountered a stale shared-cache msgq build script from a different worktree.
The rejected artifact is retained; invalidating only that script's compilation
fingerprints and rebuilding resolves the absent-file failure without changing
any source transport behavior.

The actual AGNOS 19.8-carrot-bt1 loader resolves the ARM executable and its
five-library closure offline. All required symbol versions and 562 strong
references resolve against the previously hash-verified image libraries. This
reuses those extracted files without another image download or expansion; it
does not establish execution on AGNOS or physical scheduling behavior.

Commands, rejected runs, exact executable copies, raw captures and reports are
under the parent continuation directory
`.analysis/scratch/2026-10-02-port-resume/selfdrived-arm/`. Host gates remain
sealed in the earlier receipt. Full-runtime startup/log upload, retained
project-owned C++ IPC conversion and later user device comparison remain open.
