# Repository-wide Python retirement implementation plan (#296)

> **For agentic workers:** Use `superpowers:executing-plans` for parent-owned tasks and bounded independent workers for the named families. This plan is an execution ledger, not evidence of completion.

**Goal:** Replace all maintained Python functionality in the repository, including development, build, test, model compilation and vendored tooling, with verified native Rust equivalents; remove replaced Python source only after its callers and regression coverage have migrated.

**Architecture:** Preserve independent original outputs before removing their implementation. Reuse native runtime and developer-tool libraries, port complete caller/dependency families, and verify each command's successful outputs and failure effects. A Rust command invoking Python, relocated source, downloaded old source, or precompiled model consumption alone does not replace the corresponding compiler/tool functionality.

**Tech Stack:** Rust 1.94.0, the locked Cargo workspace, native compiler/provider boundaries, existing synthetic protocol fixtures and original-source comparison evidence.

**Spec:** `docs/rust-port/design.md`, `docs/naver/rust-python-tools-retirement-296.md`, and the user's 2026-10-15 repository-wide clarification. Tracking: #296 and #1.

Current execution follows the user's 2026-10-22 grouped-validation direction in
the [functional closure ledger](rust-repository-python-retirement-296-families.md).
It maps every remaining baseline entry, additional helpers/notebooks and embedded
execution scope, names blockers and completion gates, and reuses successful
unchanged evidence. Historical per-file checkpoints below do not override that
family execution policy or relax safety/final integration requirements.

Current continuation (2026-10-23): keep Sol/Ultra parent and Sol-only independent
workers. Admit the actual publisher all-source-link repair39882326549, diagnose
new policy39875010264 and normal39878564969 failures, and reuse original109data.
Review current-cfd AST caller and bounded logger-peer/call3 recipes independently
before serialized publication. Private provider/startup/R4/loader jobs remain
pending. The readonly IO observation does not authorize an unchanged workload
rerun on missing audit prerequisites. Port the actual Python bundle builder;
do not count a native replacement of already-Bash CI guards as Python progress.
Keep all originals and the288/3/3289 inventory until actual caller/absence gates.


Historical shared-oracle correction: full PR39552825804/job119065662638 found that
`driving_daemon_reference.py` still imports `Commands` and `original_class` from
`check_desire_reference.py`. The focused deleted-checkout proof passed, but did
not cover this shared original oracle. The exact original file is restored,
SHA256 `b5529bcac8036ec559de785319b8bd131963fe05eb9aa149528992f7b4f88f8e`;
the maintained standalone checker remains Rust. Inventory is corrected to
229replaced-local/4partial/3347pending. Checkout source presence remains pinned;
the private absence proof still verifies that the native checker itself does not
execute Python. The original driving oracle must receive an independent native
replacement before actual source retirement is accepted. Earlier deletion records
below are historical focused evidence, not current repository-wide retirement.

## Global constraints

- Base: stable dev `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`; work branch `codex/feat-296-repository-python-retirement`, targeting dev through required checks.
- The concurrent `dev-md`/#338 track and its dirty primary checkout are separate. Never merge, cherry-pick, reset, stage or edit its work. This plan does not promote either track to main.
- Reuse `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`; preserve prior branches and current runtime release evidence.
- Repository-wide means current tracked source, including vendored Python, Cython, SCons and executable embedded Python. Git history and licensing/provenance remain intact. The operating system's independent installed software is outside repository deletion.
- Preserve existing functionality, failure/cleanup behavior and test assertions. No weakened tolerances, cadence, validity limits, skipped suites or self-generated expected answers.
- Count every remaining source/dependency explicitly. Native runtime success does not complete development tools, compiler functionality, Python removal or device acceptance.
- The23October user instruction removes fixed25/35GiB disk thresholds. Admit Actions builds, installs and large copies against actual available resources and estimated operation growth. All next jobs use registeredbin-actions with self-hosted/Linux/X64/dev-md-local labels, without hosted fallback. Original ARM commands run in its verified77663836 QEMU-user/PRoot guest and record physical_arm_cpu:false; emulation is not physical ARM/device acceptance. Isolate owned source/targets/toolchain caches, keep bounded build concurrency/incremental disabled, and do not interrupt another track or repeat unchanged successful checks for this switch.
- The latest23October user choice is GPT-6.1 Sol/Ultra for the parent and GPT-6.1 Sol for every subagent. Astra deliveries are finished/closed; do not launch or resume Astra. Parent owns shared Cargo manifests, workflows, deletion integration and serialized Git publication. Configured1000agent capacity is not a measured productive concurrency count; close429failures and reuse existing Sol workers.
- On2026-10-18 the user confirmed the device is unavailable and will personally pull and test normal boot, recording and upload. Finish code/test/commit integration without waiting for a connection; user device results remain separate evidence.
- Preserve the five required PR checks and separate exact merged-dev PUSH checks. No completion merely from opening a PR or scheduling CI.
- On 2026-10-19 the user moved builds, execution, original comparisons and regression validation to GitHub Actions. Local checks are limited to diff, formatting and syntax. Group related changes, inspect CI failures and continue independent ports while CI runs; do not resume the earlier local build/proof loop. Earlier completed evidence remains historical, and pending local verification entries below now mean required CI work.

## Baseline and remaining scope

The base has3,542 tracked `.py` files and38 additional Cython/SCons files. Initial `.py` locations: rust/tools1238; openpilot1017; tinygrad_repo753; opendbc_repo272; other262. These counts are inventory, not semantic equivalence or completed conversion.

Every directory remains in scope. In particular, tinygrad's tensor/compiler, autograd/training, optimizers, datasets, examples, tests and developer utilities cannot be discarded merely because deployed models run natively. Original Python runtime implementations still used by comparison tools must retain their regression value through independent fixtures/native comparison machinery before deletion.

## Review focus

1. Removed source still reached through a workflow, dynamic import, generated script or subprocess: exercise actual callers and audit execution without an interpreter.
2. Native validation accepting a case the original rejected: retain malformed input, partial output, timeout, signal and cleanup negative controls.
3. Expected answers derived from the replacement itself: preserve independent original captures, provenance and deliberate corruption/mutation failures.
4. Source classification hiding vendored, extensionless or embedded Python: inventory the full tracked tree and actual build/test entrypoints, not only selected directories.
5. Concurrent dev-md changes leaking into this branch or shared build artifacts: preserve isolated source roots, explicit branch identities, coordinated builds and exact-source receipts.

## Current implementation units

### Task1: native CI policy and bootstrap regression family

Files: new `rust/crates/dev-tools/src/ci_policy.rs`, `tests/ci_workflow_policy.rs`, `tests/ci_rust_isolation.rs`, `tests/ci_disk_bootstrap.rs`, `tests/ci_process_probe.rs` and focused support files. Parent integrates dependency/module registration and `.github/workflows/fast-checks.yml`.

Interfaces: a native YAML access layer preserving the original BaseLoader scalar meaning; policy validation against supplied workflow trees; real shell aggregator/bootstrap execution with owned temporary roots.

- [x] Preserve every original test/assertion and its input; write native baseline and independent mutation negatives first.
- [x] Observe RED for missing native behavior; implement the smallest native equivalent, retaining string keys such as `on`, scalar spelling, matrices, guards and failure states.
- [x] Run native tests and original policy suite; resolve differences without dropping checks.
- [x] Replace the CI test invocation and remove the three policy/isolation/bootstrap test files only when all corresponding cases are preserved. Retain the actual Python process-probe regressions until its active comparator is replaced; tests of a new unused native helper do not replace those regressions.
- [ ] Retain `check_changed_files.py` explicitly until equivalent Python syntax admission exists or final Python retirement makes that branch obsolete. Do not replace `ast.parse` with a lexical check.

### Task2: native Panda source-oracle build commands

Files: new `dev-tools/src/panda_oracle_build.rs`, five `src/bin/build_pandad_*.rs` entries and `tests/panda_oracle_build.rs`; parent updates `check_pandad_ci.py` callers and manifest registrations.

Interface: protocol (all five modes), USB, SPI, peripheral and runtime-fixture builders with existing output/prefix/compiler/sanitizer options and native command execution. Their output remains the same C++ oracle/fixture artifacts.

- [x] Capture independent original commands, generated includes and manifests across all modes and sanitizer variants.
- [x] Observe failing native tests before implementing builders. Preserve compiler ordering, extraction failures, disk guards, partial-output/failure evidence and absence of a false success manifest.
- [x] Compare generated bytes and normalized command/manifest outputs; build real C++ artifacts and run existing equivalence cases.
- [x] Wire native commands, verify all callers and remove the five replaced Python builders.
- [ ] Keep remaining Panda/runtime-source/Cython helpers explicitly pending until their own native replacements pass.

Additional accepted replacements and current failures are recorded in the
[evidence ledger](rust-repository-python-retirement-296-evidence.md) and inventory.
242 baseline sources are replaced-local, three baseline sources are
partially replaced because live-source helpers or unrelated Params tests remain Python,
and 3,335 baseline sources remain pending. The added
rust/tools/selfdrived_helpers_source.py retains those helpers; relocation is
not retirement. Remaining families and final delivery gates stay open.
The Bluetooth input decoder checker is now native after exact10,049-record,
artifact, raw-path, identity/cancellation and same-ELF removal proofs; the suite
caller also dispatches the retired file checker natively, with config and journal routed natively after independent original comparisons and three more source deletions. The other Bluetooth Python cases
and exact committed-head CI remain pending.
The statistics producer controller and its source helper are retired after
independent wire captures, same-ELF removal proof, six regression tests and a
fresh interpreter-free execution. Other statistics source consumers remain.

Current next batch: complete driver/startup/USB caller conversion and recovery
actions/upload/exec admission. Driving/Desire four-source actual retirement is
accepted by39663027128, with the remaining compiler export dependency explicit. Panda outer checker/test removal is integrated after153/152-source
two-root/38-phase proof; its actual deleted-checkout maintained gate39625704015
now passes with both152-source roots and all retained downstream checks. #448/#449/#450 host
regressions are accepted and affected ARM39567841179 passed; the complete PR
failed camera/navigation QA and JSON11 feature-order checks. Camera and JSON11
focused repairs now passed cloud RED/GREEN; navigation startup remains under
diagnosis. Shared Desire and its original driving consumer are now retired
after the complete two-root gate. Accepted earlier bridge, soundd, logmessaged
and Bluetooth removals remain recorded below. Remaining original helpers, model
compiler/tool families and vendor Python are still explicit pending scope. Keep
functional execution in bin-actions through GitHub Actions and inspect each exact source result.

### Task3: native model compiler operations, then the full compiler chain

Files: initial `model-runtime/src/compiler_launch.rs`, `tests/compiler_launch.rs`; subsequent units cover the exact remaining `rust/tools/model_export/` and tinygrad dependency families.

Interface: launch geometry takes original dimension types, optional local dimensions, OpenCL source and genuine compiled kernel bytes; returns the same groups/local dimensions or rejection. This is one compiler operation, not a complete compiler.

- [ ] Preserve the original launch/body results and binary provenance; observe RED and implement equivalent bounds, registers, divisibility and work-item coverage.
- [ ] Compare with genuine compiler artifacts and preserve all original rejection cases before replacing the Python operation/tests.
- [ ] Implement native ONNX/protobuf admission, metadata/slice decoding, tensor IR, operators, scheduling, memory planning, layout transforms, code generation and native provider compilation.
- [ ] Implement complete CPU/QCOM/AMD export, preparation/warp/history/recurrent semantics and immutable publication; test actual model outputs and error behavior against independent original evidence.
- [ ] Replace remaining tinygrad training/autograd/optimizer/dataset/example/test/development functionality with its own coverage. Record concrete units before implementation; none is implicitly exempt.

### Task4: remaining repository families and final deletion gate

Files: complete tracked source/dependency inventory; per-family native replacements and tests across rust/tools, tools, scripts, release, openpilot, opendbc_repo, panda, msgq_repo, rednose_repo, teleoprtc_repo and other vendored sources.

- [ ] Map each tracked Python/Cython/SCons file to actual callers, native replacement, retained behavior and verification evidence. Pending entries remain pending even if the runtime no longer selects them.
- [ ] Convert shared QA/process/IPC/fixture infrastructure and all remaining comparison suites; migrate original runtime-source dependencies after preserving independent expectations.
- [ ] Convert all remaining build/generation/documentation/navigation/Jetlink/log-analysis/replay/developer commands and their tests, preserving supported interfaces.
- [ ] Remove each replaced dependency closure only after real callers, success/error cases and native coverage pass. Preserve required licensing/source provenance as data/documentation.
- [ ] Verify zero tracked Python/Cython/SCons/extensionless Python implementations and no hidden/downloaded Python execution in maintained build/test/runtime entrypoints; run complete native suites without a Python interpreter available.
- [ ] Pass independent branch review, exact PR and merged-dev gates and final source-bound runtime packaging. Deliver the verified code for the user's requested pull/test; record boot/record/upload results when the user supplies them. Report repository-wide conversion complete only when every source-retirement requirement is met.

Remaining large units are decomposed from inspected contracts as their prerequisite families become available. Completing Tasks1–3 alone does not complete Task4 or this project.

### Active DBC generation and packaging unit (2026-10-19 KST)

The next dependency closure covers thirteen opendbc generator Python policies,
`stage_control_dbcs.py`, the Card/RadarCAN asset packagers and their three
existing packaging assertions. These seventeen baseline sources have been removed
after native verification and caller migration; their parent Python facade,
SCons and comparison controllers remain explicitly pending.

Independent original captures establish 45 generated DBCs, Card's 97-file
package and RadarCAN's 103-file package. Two shared Hyundai corner-radar files
have different bytes between the packages: Card preserves the checked-in files,
whereas RadarCAN regenerates them. Native packaging must preserve each policy.
Four original path-pattern controls also reproduced sibling-file deletion and
skipped generation when directory names contain glob metacharacters. Issue
[#375](https://github.com/bin9208/openpilot-rust/issues/375) tracks the deliberate
correction to literal directories; those destructive original effects are not
parity requirements. Ordinary generated-file equality remains required.

- [x] Capture the unchanged original generator, lazy facade, SCons callers and
  both packaging commands, including CLI/API errors and raw-path behavior.
- [x] Observe missing-generator API RED; prepare native generator implementation
  and ten API/six CLI tests from independent captures. The optional machine-error
  boundary retains raw filesystem/Unicode bytes for the still-pending facade.
- [x] Pass the generator tests, strict Clippy and actual native output checks.
  All ten API/six CLI tests pass. A frozen executable with unavailable PATH
  matches 45 original files/992,535 bytes and traces only its own exec call.
- [x] Observe missing-packaging API RED, implement both packagers, and preserve
  failure ordering, source isolation, atomic Card publication and partial Radar
  output. Nine API, seven CLI and three raw-basename tests pass. Lossless JSON
  preserves U+DCFF keys. Original failure/sort captures exposed FIFO blocking
  and whole-path ordering differences; failing native controls preceded the
  nonblocking file admission and Python-compatible component-order repairs.
- [x] Verify the migrated lazy/SCons/CAN/Card callers and mandatory CI paths locally.
  Source edits add seven build/test disk guards while retaining the original
  177 guard arguments; 54 current CI/disk/lagd tests pass. The newly guarded
  controls job increases covered Rust jobs from 33 to 34.
  Relative native-tool overrides now resolve against the repository consistently
  across SCons and runner cwd changes; source-level path controls passed.
  Actual lazy cache/error controls, three real private SCons builds and Jot
  materialization callback pass. Seven bootstrap process-boundary tests pass,
  as does real default SCons bootstrap across generation, newly discovered
  dependencies and third-run no-op. This is not a full application build.
  Independent review found two executed bridge source files missing from CAN
  receipts; both now participate in their shared provenance calculation.
- [x] Remove the proven seventeen-source closure, repeat output/caller checks
  without a Python execution path, then update the retirement inventory.
  After deletion, the actual native generator and both packages again match
  45/97/103 original files; their unavailable-PATH traces contain only their own
  executables. Actual facade/SCons/Jot callers and all eight Card fixture stages
  pass. These retained Python callers are not counted as retired.
- [ ] Complete exact committed-head CI, including supported-platform validation,
  before integration. Linux-only pidfd process QA now has explicit platform
  gating so the DBC host tool does not inherit its Linux compile requirement.

Private original captures and source-only gate plans are in
`.analysis/scratch/2026-10-18-dbc-generator/`. Builds use a single coordinated
compiler lease; intermediate passes do not establish the complete retirement.

Docs-Not-Needed: internal engineering plan; no user setting changes.

### Follow-up compiler unit: pinned native LLVM provider and IMAGE scheduling

Additional independent IMAGE controls found output-layout-dependent signed-zero
behavior in the original LLVM optimization pipeline. Original emitted IR starts
at +0 and uses `nsz arcp contract afn`; unrolling/SLP sometimes removes that initial
zero and sometimes retains it. A channel/count-based C accumulator shortcut is
not equivalent and remains uncommitted. Existing positive tests stay intact.

- [ ] Add an isolated `offline-llvm-compile` native worker using the existing
  libloading dependency, explicit library SHA, target triple/CPU/features,
  pass pipeline and unroll/vectorization/SLP/verification configuration.
- [ ] Verify module admission, per-instance ownership/cleanup, bounded child
  execution, failed-output cleanup and source/object/optimized-IR receipts against
  independent original CPULLVMCompiler results for configured LLVM18 and20.1.
  Same-version/context equality is required; cross-version equality is not claimed.
- [ ] Port parameterized IMAGE packing and reduction/output loop scheduling from
  the retained source algorithms, preserving four-lane layout, padding/undo and
  initial +0. Emit LLVM with the original flags and let the configured optimizer
  make its decisions; do not select behavior from observed channel/width lists.
- [ ] Integrate objects through the existing artifact ABI only after reference
  cases, earlier positive coverage, actual model prefixes and full model checks
  pass. Keep generic normalization, other schedulers and full compiler removal
  explicitly pending. Wire mandatory CI provider validation before publication.

### Follow-up UI units: source polygon reference and image comparison

The independent Rust polygon reference uses a narrow raw raylib ABI and preserves
editable scenes, frame order, shader policy, state artifacts and failure phases.
Actual original/native host PNG/state comparisons are intermediate evidence;
source-helper retirement still requires review repairs and caller migration.

A separate decoder unit preserves `PIL.Image.open`/NumPy raw modes, dimensions,
palette indexes, integer conversion and broadcasting. The original checker accepts
non-PNG content under `.png` names, including TIFF/TGA/DDS/GIF/SGI/IM, and some
palette/16-bit broadcasting inputs. RGBA normalization or PNG-only decoding does
not preserve those admitted cases. Independent codec arrays and comparisons must
close proven gaps before retiring the checker. No Python wrapper or discarded
format coverage counts as a native replacement.

2026-10-18: codec comparison CLI is native;14original failure/artifact contracts,
5actual codec cases/112packets,48post-removal tests and strictClippy passed. Original
codec checker removed and CI caller/build/fingerprints migrated. Runtime/V4L and
UIapplication/model compiler remain pending. Inventory111local/2partial/3467pending.

2026-10-18: V4L checker replaced by native check-encoder-v4l;17original main and
19outcome captures,32actual source/Rust cases,51post-removal native tests and strict
Clippy pass. Codec path-resolution failure phases repaired against four unchanged
original CLI captures. Inventory112local/2partial/3466pending. Exact8752 UI pixel
failure and full application/model/remaining tool closure remain open.

2026-10-18: application lifecycle checker and independent reference are native;
two original tools removed after actual six-mode proof, repeated after deletion.
Native frame/recording/input/resource policy and failure guards remain mandatory.
Inventory114local/2partial/3464pending. Runtime encoder/CI/model closure continues;
no repository-wide completion or device acceptance is implied.

2026-10-18: native encoder host CI/runtime/outcomes closure passed actual5codec,
32V4L,10runtime comparisons without a Python executable on PATH. Five Python
files removed; four unrelated Params tests retained unchanged with a required CI
caller, so their baseline row remains partial. Post-deletion86native tests and
strictClippy pass; post-deletion47case host proof also passes. Remote exact-SHA
gates remain pending.
Inventory118replaced-local/3partial/3459pending. Target-artifact validator and
model compiler remain active next units, separate from physical user acceptance.

2026-10-18: ARM encoder validator replaced by native check-encoder-target after
five original/native codec cases and112packets; post-removal103native tests and
the actual ARM gate pass,234trace/packet files remain byte-identical. Separate
file logging, executing-checker identity and rawUnix argument failure receipts
are covered. Inventory119replaced-local/3partial/3458pending. Model/compiler and
remaining tool families remain open; user owns physical device acceptance.

2026-10-18: Timed QA thirteen Python entrypoints/helpers were removed after the
eight native checkers matched493 original cases. The same frozen tools passed
all493 again after deletion, with1,025 execve attempts and zero Python in each
run. Post-deletion118tests and strict Clippy pass. Mandatory CI callers now use
the native commands. Inventory141replaced-local/3partial/3436pending.

2026-10-18: Seven checkout QA tools are native after full-nine original/process
comparisons, twelve failure phases and arbitrary-Unix-path/environment controls.
Ten tests, strict Clippy and all frozen runtime proofs pass after deletion;
426 exec attempts in each final complete trace contain no interpreter.
Inventory148replaced-local/3partial/3429pending. Exact-commit CI remains required.

2026-10-19: native Version controllers preserve171 dynamic comparisons, actual
two-service/one-disk collector delivery and failure/owned-cleanup boundaries.
After collector Python removal,96tests, strictClippy and fresh original/native
proofs pass. The shared reference loader remains partial until tombstoned and
managed-entry consumers migrate. Inventory179local/4partial/3397pending; exact-SHA
CI, dev integration, runtime release and user device acceptance remain separate.

2026-10-19 continuation: the independently captured three-encoding SDK HTTP checker
is now native and its standalone Python caller is removed. Inventory is180local,
4partial and3396pending. Shared crash transport/managed-entry callers remain. The
independent Sentry source Worker passes39 foundation plus72 stateful controls;
full242-call source coverage and controller closure remain active work. Latest
exact-SHA CI, dev integration, source-matched release and user-owned boot/recording/
upload verification remain required delivery gates.

2026-10-19 managed-entry continuation: all44 original source-side results from
exact-head CI artifact12146936832 are preserved as selected field contracts
(`tests/fixtures/managed-entry/original-fields.json`, SHA6b0ca0e35bbc9bb51b2ff78e9560aefbed5bc40e8d65a072346cad69d1251271).
The two collector paths agree across all22scenarios. Native migration must retain
actual context replacement/bodyIPC, both collector paths, source policy, Params
error/SDK ordering, wire/disk identity and externally captured SDK fields; fixtures
alone do not replace those live checks. Unknown selected case falsePASS is issue381.

### Registration clock and CI budget continuation (2026-10-19)

Removed the standard-library registration clock checker after same-ELF before/
after13-case, CLI/error/raw-path and native-exec proofs. Independent source policy,
input-drift/stale-PASS repairs and fresh16-test/Clippy/build checks pass. Inventory
188local/7partial/3385pending. Four previously partial telemetry helpers remain
present while their new absence/provenance/cancellation controls are validated.

#390 moves the exact29 LLVM20 targets into an independent required job; no target,
provider hash or assertion is removed. Native CI62 passes; exact remote validation
and observed wall-clock improvement remain pending. Upload consistency defects
#388/#389 have native REDs and are being repaired separately. Device checks remain
user-owned, and repository-wide conversion/final integration remain open.

### Active continuous statistics validation closure (2026-10-19)

The next parent-owned unit covers `check_stats_daemon.py`,
`check_stats_failures.py` and `check_stats_log_fault.py`. The latter two checkers
import the daemon checker's line reader, so retirement requires all three
callers together. `stats_source.py` remains because `check_stats_interrupted.py`
still executes it. The independent original `statsd.py` and interrupted-receive
consumer remain outside this removal unit.

Unchanged original/native executions passed ten scenarios. Source-only captures
preserve continuous flush/state transitions, all680 Unicode15.0 decimal inputs,
10,000-file capacity/discard semantics, eight filesystem/Params/input failure
boundaries and the first-log-error fallback. The native controller will run
those exact scenarios against actual daemon/producer examples, the existing
independent source collector, real Params files and msgq/ZMQ transports. Fixed
captured results retain the controllers' fixed input domain; they do not claim
to replace arbitrary statsd behavior or a general-purpose source runtime.

- [x] Capture exact original results, full inputs and source provenance before implementation.
- [x] Observe the new controller API tests fail, then implement the native real-process runner.
- [x] Preserve deadlines, log ACK barriers, file permissions/content and original error outcomes; run cancellation, input identity and FIFO controls.
- [x] Pass focused tests/Clippy and unchanged original versus native live comparisons.
- [x] Wire all three CI callers, demonstrate same-ELF interpreter-free execution before/after removal, then rebuild and retest before recording retirement.

This unit consumes the already-verified independent collector transport; it does
not change its policy or the deployed statistics daemon. Compiler use remains
serialized with the registration, proclog and soundd units.

### Completed interrupted statistics receive closure (2026-10-19)

The former `check_stats_interrupted.py` consumed `stats_source.py`; both are now
removed after the paired retirement gate. Fresh original-only runs with a freshly compiled
C fault shim passed all five sequences: one/repeated EINTR, EINTR then EAGAIN,
EINVAL and ENOTSOCK. Preserve the exact trace faults and DONTWAIT flags, retained
metric flush, fatal exit/no-file effects,5-second clock/delivery/wait bounds and
explicit historical `--before` behavior. The reference fixture is source-only;
no candidate supplies expected output. The original collector/Params/msgq inputs
remain source-pinned, with actual native process and filesystem checks.

- [x] Capture five original scenarios and current C shim/source identities.
- [x] Observe API tests fail before implementing the native controller.
- [x] Run fresh native link-wrapped candidate, all five traces and ownership/failure controls.
- [x] Route CI through the native checker without weakening its C wrapper or assertions.
- [x] Prove same-ELF execution before/after removing both Python files, then rebuild/retest.

The existing three native statistics controllers still pass after shared-helper
retirement. The frozen eight artifacts passed 15 cases per phase, with 50 exec
attempts and no Python in each phase. Fresh post-deletion verification passed
72 tests (7 interrupted,8 predecessor,44 CI policy,13 disk) and strict Clippy.
The separate real-clock and busy-producer runtime checkers remain pending.

### Active registration/navigation helper closure (2026-10-19)

Retirement candidates are `check_registration.py`, `registration_source.py`,
`check_navd_config.py` and `navd_config_source.py`. Navigation imports the
registration helpers, so this is one dependency closure. Other navigation and
startup tools retain their existing bindings and source requirements.

Independent registration captures cover the full131-case baseline,3 real socket
timeout cases, generic inputs and primitive text/response encoders. Navigation
captures cover26 named configurations,8 generic inputs, CLI/process failures
and two additional GPS error-order controls. Preserve live key generation,
signatures, Params, collectors and real timing; expected results must not come
from the candidate. Source hashes and historical absent hashes are distinct.
No retired Python is retained under a renamed source/text extension.

- [x] Original-only capture and actual pre-implementation API failures.
- [x] Frozen registration13 tests and independent primitive4 pass.
- [x] Add actual CLI test for both delayed collector schedules; it passes.
- [x] Observe missing-helper guards and closed-stdout stale-PASS regression fail.
- [x] Final native CI routing passes 46 policy and 13 disk tests, including all retirement targets and receipt-owner units.
- [x] Exact absence adapters, output failure fix and final full native gate.
- [x] Same nine artifacts before/after all four removals: 27 tests, 134 registration cases, 26 navigation cases and 17 CLI controls.
- [x] Post-deletion compiler phase finished before the workflow change; all nine outputs are byte-identical to the proven frozen artifacts.
- [ ] Exact committed-SHA CI; no further local runtime repetition under the new workflow.

Navigation's relative-path test expectation was corrected without changing the
parser. The original GPS `.get` error order required a source-adapter correction.
The output failure fix preserves per-case evidence while withholding a fresh
success aggregate on broken stdout. The new retirement target remains mandatory
alongside navigation's existing tests. Startup's45-minute budget retains the
previous28m29s workload plus the added native controls.

### Production statistics runtime/flood tools awaiting CI (2026-10-19)

Removed `check_stats_runtime.py` and `check_stats_flood.py`; shared original
logging helpers remain. Fresh production `statsd-rs` and `stats_flood` artifacts,
sealed against373 source inputs, passed the original real-clock case, both
busy-producer signal cases and16 CLI/effect controls. The original flood checker
accepts an existing root while creating new per-signal children; preserve that
behavior and unrelated files. Missing native API E0432 was observed before
implementation. Keep the10-second admission,5-second normal shutdown and2-second
busy shutdown bounds, real timestamp window,0600 output,10 user CPU ticks and
two live producers. Before the workflow change, 24 native tests, strict Clippy
and the same-artifact three-case proof passed both before and after deletion.
Fresh post-deletion builds and regressions now run in the required CI job.

### Next grouped cloud batch after 1364e48c

- Repair the exact-output relocation guard, bridge report ownership, logging
  Clippy expression and soundd generated-module spelling from actual CI logs.
- Add native calibration Float32 and Panda safety-padding controllers, preserving
  both unchanged originals and their real compiled probes as mandatory controls.
  Keep their source retirement pending until reviewed cloud comparison results.
- Reproduce and repair modem helper EOF, descendant and cancellation ownership
  under #398 using private processes; retain the original modem comparisons.
- Local work remains source/format/syntax only. At 1364e48c navigation passed and
  the previously failing statistics comparison step passed; neither establishes
  aggregate success for that run or for the following source changes.


The next three actual removals retire the beep reference controller and the calibration estimator controller/history generator after accepted same-current-ELF private-absence proofs (39541676860 and39541772800). The unchanged calibration CLI was freshly compared before deletion. Maintained callers now select Rust. Actual deleted-checkout cloud validation is required before accepting this batch; shared original helpers and all remaining Python remain in scope. Inventory226local/3partial/3351pending is source implementation progress, not project acceptance.

### Updater full controller and original regression retirement (2026-10-21)

The exact505cc30c pre-removal cloud run39543788680 passed16 unchanged Python
regressions,15 Rust tests including all16 original cases, five fresh source/native
scenarios,11 sealed CLI admissions, strict Clippy and two strict1,089-execution
native-only root audits. Parent and independent worker inspected the downloaded
artifact. The maintained startup-services caller now builds the native feature and
runs full regressions/Clippy/default CLI, preserving process/daemon/AGNOS gates.
Removed only check_updated.py, updated_source.py and test_updated_comparison.py;
updated_fixtures.py and the other pinned original dependencies remain. The
actual-deleted-checkout run39547256419 at083f937a passed15tests/strictClippy/default
CLI and both1,089exec audits. The whole startup-services rerun remains pending.
Inventory229replaced-local/3partial/3348pending; entire project acceptance is open.


### Desire complete original controller retirement (2026-10-21)

Run39548303946/job119051425025 checked1de0b0d7 before removal:83lib,
4integration, full24585-row same-ELF checkout/private-absence corpus and Clippy
passed. All20 original/native CLI admissions have matching exit statuses and
channels, including successful raw0xff output paths. Help/error wording differs
and is retained as evidence rather than claimed byte parity. Removed only
check_desire_reference.py and replaced the real workspace caller with mandatory
native unit/integration/CLI/absence/default-CLI gates and full artifacts. Shared27
original dependency pins remain. Actual-deleted-checkout cloud proof remains
pending. Inventory229replaced-local/4partial/3347pending.

### Native estimator comparison and controlsd fixture closure (2026-10-23)

Integrate the six-file paramsd estimator controller and seven-file controlsd
fixture generator as opt-in native tools. Reuse the accepted Paramsd seven-case
input generator, Pose/CarParams and prior loop/daemon/startup scopes. Preserve
original Python helpers as independent Actions oracles until the new full-schema
comparisons and their maintained callers are admitted. No inventory credit yet.

Estimator compares1196fresh original learner rows, exact Event shape/times/logs
and existing asymmetric NumPy tolerances; focused mutation, process/cancellation
and independent NumPy boundary controls precede acceptance. Controlsd builds the
actual16-service roster, shallow mutations, vehicle settings/PSA tuning and all
30cases/8605frames; compare complete original projections and float bits without
changing runtime arithmetic. Parent owns shared Cargo/lib registration. The only
Cargo.lock delta is existing workspace openpilot-card in dev-tools dependencies;
no external package/version change is allowed. Independent source reviews and
registered bin-actions build/original/native gates remain required.
