# #296 current implementation handoff

Current execution state, not completion. The [evidence ledger](rust-repository-python-retirement-296-evidence.md)
retains exact original captures, failed attempts, sources, workflows and artifact
SHA/CRC audits; Git retains prior handoffs.

## Scope and boundaries

- Work only in `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`,
  branch `codex/feat-296-repository-python-retirement`, draft PR343→dev.
  Preserve primary dev-md/#338 and `.omo`; no protected merge/release/deployment.
- Parent6.1Sol owns integration/review, registration/workflows, deletion and Git.
  Latest user permits6Sol/6Luna workers by complexity; reuse existing ownership.
- Local syntax, scoped rustfmt/diff and stored-data inspection only. Builds,
  functional/original tests, Clippy and ARM use GitHub-hosted Actions.
- Reserve25GiB plus growth; recover35GiB below floor. Two build jobs, incremental
  disabled. Local free space is about93GiB; keep needed current evidence only.
- User has no device and will pull/test boot, recording, upload and driving.
  Do not request USB or claim device acceptance, CPU savings or an AGNOS release.
- Continue development/build/test/vendor/notebook/embedded conversion. A Rust
  launcher around Python does not retire the underlying Python functionality.

## Accepted inventory and global gates

**259replaced-local /3partial /3318pending of3580 baseline entries.** Additional
helpers/notebooks are separate;37observed embedded execution sites remain pending.
The [functional closure ledger](rust-repository-python-retirement-296-families.md)
and exact pending routing TSV retain ownership/dependencies. No source count is
an all-module clearance or complete runtime/device acceptance.

Navigation startup #457 and historical PTRACE_LISTEN/EIO #474 remain unexplained.
Generic ARM/QEMU is not a sealed AGNOS/device candidate. Keep #296 open.

Published current PR headd61079461c0e217056095fcd56d6831230317137. Complete current
Integration39703167608 and Fast39703167593 pass. Rust39703167508 has the known
#476 model failure; Card119576923165 passes and workspace119576923681 is running;
inspect actual latest results before acceptance. Previous f913de3aa checkout is
synthetic merged39b1b08c2079e9b3356be06ddddf9db49551be6, independently same tree.
Integration39691148113 passes. Rust39691148023 is complete:44successful jobs,
including generic aarch64119537355478, four failures plus failed aggregate.
Failures were workspace formatting/metadata binding (#475), Panda source roster
(#477, now closed after complete focused proof), old Card policy (#475) and model
numerics (#476). Fast39691148018 failed old host-staging assertions (#478).

Source fixes for the CI contracts are committed. Current-source complete Fast
routing/metadata/Card39696562943 atdf374b0ab passes, including77isolation checks,
3host-stager controls,13disk/6workflow policies and all67metadata observations.
Corpus postdelete39698084823 additionally passes whole formatting and78isolation
checks after the new Kalman step/policy. Full Fast39703167593 confirms all78
isolation checks and closes #478 (comment6330242277); #475 still needs final complete workspace. Diagnostic-branch automatic Fast runs
inspect a different tree and are never evidence for their pinned source checkout.

## Accepted focused closures to reuse

- Recovery13:39687004413 at57594f054, eight controllers in two actual absent
  roots, strict Clippy/ownership/policies. Parent audits7346CRCs,2508trace files,
  4795exec attempts (2393success/2402explicit failure),96created bodies and two
  52-row held alias receipts. Observed Python exec0 in those lanes; raw argv is
  retained without claiming argv parity. server.py/repo_update.py remain live.
- SoundD staging:39688832018 at7219cf5e7. Original/native20files/173533B,21pins,
  five controls/Clippy, seven audio modes in both roots; each runtime lane has
  22closed native execs/Python0. Other audio Python remains pending.
- Metadata:39681730943 at9076ccc41,14actual maintained callers/real host binary,
  67original cases (34success/27errors/6explicit schema), seven units and strict
  Clippy/policies. Preserve raw paths and measured filesystem enumeration order.
  Current workspace executable binding is rechecked39696562943; parent audits
  all70CRCs and independently recomputes all67cases/order projection.
- Driving/Desire actual four-source deletion:39663027128 at2ecf3f681. Two absent
  roots, model/collector/fault lanes,24585rows/47scenarios and eight reappearance
  controls. Preserve independent source data and keep deleted shared helpers gone.
- Teleop:39670630661 plus maintained policy39674181956;34computations/10disposable
  Git effects. No real repository tag/remote effect is inferred.
- Panda device39640526320 and protocol/CAN-I/O39647358174 remain accepted.
  Updated complete outer39698464972 at6c6733f84 now passes all packages/Clippy,
  14controller/3retirement controls, **two current native roots×38phases**,
  42disk receipts per root and34held ELF bindings. Correct source roster163:
  151historical+17helpers+1manifest-5phase sources-1orchestrator. Parent audits
  17095CRCs,163Git source hashes,76phase records/argv and84disk receipts.
  Earlier independent original evidence is retained; live Python phase helpers
  still run. #477 is closed with comment6329891483; no additional inventory count changes.

## Kalman four-command development closure accepted

Native equation generators use a typed Rust symbolic DAG, analytic derivatives
and deterministic Rust emission; they do not copy the independent expected files.
Original capture39692474248 atf913 has30files/42input dependencies; parent matches
3298Python/schema source hashes and all30output pins. A zero-speed Car derivative
failed original-inf/native-NaN; algebraic denominator simplification fixes it
without changing values/tolerance. Predelete39693981650 passes758function rows,
constant bits and singular/NaN/infinite boundaries. Actual deletion39697287904 at
df374b0ab passes same-ELF actual/empty-cwd emission, full11/19-file rosters and fresh
compiled comparisons. Equation2 entries accepted; installed modules unchanged.

Native corpus tools retain Pose16/Car24 PCG64 inputs and evaluate the explicitly
selected original C++ oracle. Private FFI validates ABI/exact widths, holds header/
ELF and serializes C++ global setter/load/call/drop lifetimes. Dev-tools denies
unsafe except that private boundary. Original39696759740 at30f80e2f6 proves both
full JSON outputs equal fresh Python and existing independent runtime corpora.
Postdelete39698084823 at2e83f1dba passes current writer, same-ELF actual/empty-cwd,
full byte equality and symlink/hardlink inode/mode behavior. Parent audits113CRCs,
4047Git source hashes, six successful CLI/stdout receipts and provider seals;
independent review corroborates link/cwd receipts. Corpus2 entries accepted.
Original symbolic definitions, C++/Cython builders and other consumers stay pending.

Both commands are maintained in required workspace/estimation jobs. Eight new
capacity markers raise exact guard count517→525; existing budgets are preserved.

## Driver/startup/USB six-source deletion still pending

Maintain separate4-model driver and5-model family data/catalog receipts. Only the
metadata PKL matches; do not substitute datasets, relabel CPU targets or reuse
old compiled kernels. Existing model compiler remains Python and pending.
Original startup/USB capture39680582209 uses immutable runtime-b55552db data:
45assets+3licenses, original e758b96d model/a25b81e9 runtime/1bb0c80c manifest.
USB original controller exits0; its wrapper fails only a late unused-msgq inventory
assertion. Preserve explicit incomplete-wrapper status and unknown ELF lengths.
Both fixed manifests/114data files stay byte-preserving; a CRLF normalization
failure was repaired without changing pins. Full native prep39685067872 passes.

Graniterapids first nonuniform frame fails #476 in39689390187 and full119537355539.
Bounded original Python/model-run39694362489 onznver4 matches all warped/model
bytes and frozen predictions. Original collector provider is **znver3/EPYC7763,
LLVM18**, not earlier unusable Intel objects. Current same-host39696467591 on
znver3 matches daemon/model-run/frozen complete2212B warmup/frame0 predictions;
all47IR texts and both1382400B warped outputs also match znver4. This does not
explain the Intel failure. Do not relax tolerances or replace original expectations.

The znver3 gate then exposes #479: driver Collector receives historical capture
commit2ecf instead of authenticated current checkoutdf374. Candidatea29832fd0
carries current checkout only to live logging; all historical manifest pins stay.
Active grouped/dedicated regressions bind every collector report to the current
checkout, and exact log-context rejection is unchanged. Retry39699899580, workflow
94db924b4f2acc24e29ca250ffe5e4bdc8938b00, passes all16driver/8startup and emits all6
USB passed reports in actual root, then fails final original-held input ctime
verification (#480); private root is not reached.5b5aa357b isolates case links
from original inputs using one independent copied pair. Focused39707683464 passes
regression+strictClippy; USB-only39708204175 atsame5b source is running both roots.
This is not full-group/#479/#476 acceptance.

Failure-only native postmortem stays diagnostic/non-acceptance and preserves gate
failure. Two actual hosted contexts39700512883/workflow0e522bf2a prove EPYC7763/
znver3 matches, while EPYC9V45/generic withAVX512 reproduces58456ffe without the
daemon. Original Python still matches frozen99b8b2fe; both full warps and47IR
texts match. Parent audits both258-CRC ZIPs and all raw predictions (439/553float
values differ, maxabs0.01171875). This is not Intel-specific. Static review sees
original PROGRAM.src[4] retained code versus export recompiling src[3] IR for the
current host; target/codegen causality is not yet established. First divergent
kernel diagnostic39707504533 atd610 completes valid88call/two-frame comparison
in both samples, but both actual hosts are EPYC7763/znver3 and return
NO_PER_CALL_DIFFERENCE. This does not diagnose the mismatching host. Natural-host
preflight retry39708871859 skips only exact already-matching non-AVX512 EPYC7763
hosts; other hosts run unchanged comparison. No target forcing or old-object
substitution is a fix. Preserve strict independent expectations.
Keep #476/#479/#296 open until their actual criteria pass.

## HardwareInfo next-family candidate

Original hardware119537355450/run39691148023 passes136cases/3110observations/
70datagrams. Parent selects955source members from3GB artifact12388879432 with
bounded206ranges (7.4MB network,37.6MB selected expanded); no full-ZIP claim.
Original-plan/9CLI admission capture39702167583 at3ddae8/workflow10185ac563b237e73696c7a4d90f7fec19d04ce4
passes;25CRCs/SHA3b61d990524bf00b749f2c9b550f8a4ce39b16ad395d71a0d52847522922bb1f.
Parent matches all136 source request schedules and70wire bytes,28original source/
binding Git hashes and12plan-source hashes at both captures. Current fixtures
are source-only gzip byte preservation,974members with compressed/expanded pins,
30source hashes; manifestSHA2a0296cd351c530457f7fc9a3219176f0e533eb30a5da325a5bc1f0307a545af.
All974 committed Git bodies preserve pins. Historical inherited env was not directly
recorded; explicit later env must reproduce complete historical path observations.

Native leaf/capture/CLI registered148030, runner+outer review094f and lint891425.
17c770c72 fixes the original thermal-suffix-+02 name admission and avoids repeated
~40MB expansion on final verification while preserving exact compressed identity.
39707436994 passes units/strictClippy then finds only filesystem failed-read order
a,b versus originalb,a in keyfile-read-errors. Both fail with identical diagnostics
before same fallbackc; values/wire match.212721e1b admits only these two failed-read
orders in this case with exact diagnostics, retaining raw logs/explicit receipt;
all other ordering remains exact. It also retains source receipts through fallible
stdout and provisional manifest finalization. Full hosted retry is pending.
All3original tools remain live; count stays259/3/3318.

## Hardwared candidate in progress

Original artifact12389225409/run39691148023 is fully audited3984CRCs/1029617B,
SHA4285c91ffd0b45568d79472a782c8ca05a4cd475eca8cb7ce196af65d74f911b.
Captured source.json264722B has12cases/891inputs/890expected observations. Parent
verifies gzip exact equality and19oldGit/current source pins. Native daemon IPC
artifacts are engineering baseline only, never frozen expected values. New capture,
trace/live peer leaves, native CLI wrappers and actual/private outer are being
integrated. Featurehardwared-qa-native uses existing nativeIPC dependencies. All3
original tools remain live and pending hosted proof; five original runtime files
and shared schema/import/Python bindings remain separately pending.

## Publication next

Keep source changes and `.omo` intact. Publish a coherent issue-branch batch and
update draft343/#296; final complete exact-source required/workspace/ARM results
remain necessary. Do not stop at queued CI or a PR, merge protected branches or
claim device acceptance. Latest #296 checkpoint6330165232 and draft343 describe publishedd610, four Kalman
closures, corrected Panda, AMD reproduction and pending current complete gates.
