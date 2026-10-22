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

Published full PR headf913de3aa3c0f2c261f4ae4b87e41f3eb317fb82 checks out synthetic
merged39b1b08c2079e9b3356be06ddddf9db49551be6; parent verifies identical trees.
Integration39691148113 passes. Rust39691148023 has43successful jobs,4functional/
policy failures plus failed aggregate; generic ARM remains running. Failures:
workspace module format/empty metadata executable (#475), Panda missing source
roster (#477), old Card aggregate policy (#475), and model driver numerics (#476).
Fast39691148018 fails inherited host-staging assertions (#478).

Source fixes for the CI contracts are committed. Current-source complete Fast
routing/metadata/Card39696562943 atdf374b0ab passes, including77isolation checks,
3host-stager controls,13disk/6workflow policies and all67metadata observations.
Corpus postdelete39698084823 additionally passes whole formatting and78isolation
checks after the new Kalman step/policy. #478 still needs final complete Fast;
#475 still needs final complete workspace. Diagnostic-branch automatic Fast runs
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
  still run. #477 acceptance is met; no additional inventory count changes.

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
94db924b4f2acc24e29ca250ffe5e4bdc8938b00, is running at that source.

Failure-only native postmortem stays diagnostic/non-acceptance and preserves gate
failure. Additional diagnostic39700512883/workflow0e522bf2a uses two actual hosted
contexts, each with both warmup/frame0 sequentially, fresh native catalog and
unchanged original Python pipeline. It records actual CPU/provider and decodes
sealed frozen predictions only as diagnostic classification. Never force a CPU
or present the diagnostic as six-source acceptance. Keep #476/#479/#296 open until
their actual criteria pass.

## Publication next

Keep source changes and `.omo` intact. Publish a coherent issue-branch batch and
update draft343/#296; final complete exact-source required/workspace/ARM results
remain necessary. Do not stop at queued CI or a PR, merge protected branches or
claim device acceptance. Latest #296 checkpoint6329291714 predates the four-command
acceptance and current #479 evidence; replace with an accurate new checkpoint.
