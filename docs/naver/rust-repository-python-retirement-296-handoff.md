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

**270replaced-local /3partial /3307pending of3580 baseline entries.** Additional
helpers/notebooks are separate;37observed embedded execution sites remain pending.
One previously missed modem reference site is now explicitly recorded as closed;
it increases recorded closed edges24→25 and does not reduce those37pending sites.
The [functional closure ledger](rust-repository-python-retirement-296-families.md)
and exact pending routing TSV retain ownership/dependencies. No source count is
an all-module clearance or complete runtime/device acceptance.

Navigation startup #457 remains open pending published full integration: the
num-bigint-dig development/test opt-level3 candidate passes the complete focused
navigation suite and eight fresh starts at39724315701. RSA generation is
142.57–419.92ms; the unchanged10s barrier, RSA2048 and release profile remain.
The older uninstrumented failed phase is unproved. Historical PTRACE_LISTEN/EIO
#474 recurs in current full workspace119628631200 and remains open.
Generic ARM/QEMU is not a sealed AGNOS/device candidate. Keep #296 open.

Latest published PR checkpoint is4d599dde6ec461cc43089d6f87381631aabdee6c.
Current full Fast PR39719156487, Fast push39719148455 and
Integration39719156492 pass. Rust39719156536 has four failed functional jobs: driver-family numerics#476,
original-pipeline raw prediction comparison#482 (one signed-zero bit), navigation
startup#457, and workspace recovery tracing#474 (PTRACE_LISTEN/EIO). Workspace
Cargo tests and strict workspace Clippy passed before that later tracing failure;
remaining steps were skipped. Generic ARM remains running. UI/connectivity and
LLVM20 pass. This is not complete workspace or AGNOS/device acceptance. #478 and
the QA-only RadarCAN FD race#481 are closed with focused/full evidence; #480
remains open despite its focused two-root USB pass. Six model-controller
deletions remain unaccepted.
Prior faeef14da43d3a2c59ec71d7966e191828a748b3 was validated
with tested synthetic merge939a190a11fc991a4eaff958d514dc2fca5c07e2 (baseb55552db).
Rust39713242787 exposes four distinct current concerns: workspace stager policy
#478 (22 actual callers versus old23; repaired), RadarCAN ARM provenance FD
race#481 (repaired), driver model numerics#476, and a newly isolated driving
model raw signed-zero difference. Full Integration39713242775 was canceled,
not accepted. User-doc runs39713330818/39713242867 pass. Complete published Fast,
workspace, ARM and integration checks remain required after the current batch.
Earlier d610 Fast/Integration and generic ARM passes are earlier-source evidence.

Focused full Fast routing39714146176 at422671ae5 and39716711789 atfc0f017bd pass,
including13disk controls/78isolation checks and corrected22native-stager policy.
#478 was reopened after the faeef regression; focused success alone does not
close it by itself. The current published full Fast closes#478.
Diagnostic-branch automatic Fast runs inspect a different tree and
are not proof for their separately pinned source checkout. Hardwared postdelete
platform39717847363 repeats the complete maintained job with the original root
RUSTUP_TOOLCHAIN=1.94.0; parent full4165-member CRC/SHA/source/CLI audit is accepted.
First39716711789 also
passed the whole platform job but its diagnostic copy omitted that root env.

Modem five-tool deletion is accepted by complete postdelete platform39727029012
at da3483724 and policy39727105631. Full6623CRC artifact12404265956 SHA
76da7b3b3a0fdaa614066b7f02bb4f1cee3096932631d5a212b3fff4e2dc5dad retains
13cases per root,12CLI/6reappearance/4unchanged ELF receipts and all manager/
#398/hardwared gates. Parent recomputes23Git pins and all original/native case
results across both roots. Runtime modem.py remains live/pinned for its Python
manager caller; runtime-wide retirement_acceptance remains false. #483's stale
filter false green is repaired with exact success-line and1passed assertions.

Native manager QA candidate83fb19a8c adds three CLIs, seven independent cases,
four actual ZMQ log packets, held-file identity/content guards and positional
compatibility. Hosted39728775823 passed compilation/strictClippy; functional
comparison is pending. No manager Python source is deleted. Original capture
39726612260 has full7331CRC/Git3271/source/provider3495 seals; the current native
run expands selected scenarios to normal,normal,custom. Car-name generation
and complete imported-source/private-root closure remain separate work.

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
stdout and provisional manifest finalization. Full predelete39709448191 atb0e77aa01 passes; see accepted postdelete below.
All3original tools are actually deleted97c33434c. Full maintained hardware
119601261069/run39710753791 passes: both actual/private require3absent, each
136/3110/70, unchanged input receipts and same native ELFs. Parent audits complete
162436985B artifact12397456757/all12389CRCs/SHA382b801c8ca8f95d6865c36f046703aaa3978be9c1037f7b1dfa1c24cbd30b6a;
retains3028HardwareInfo members. Corrected policy39711275087 at028919c8d passes
13disk/78isolation/wholefmt; four new reserves separately preserve the old177
entry aggregate/hash and total529 markers. These3entries are accepted:262/3/3315.
Remaining original HardwareInfo runtime sources/Params binding stay pending.

## Hardwared candidate in progress

Original artifact12389225409/run39691148023 is fully audited3984CRCs/1029617B,
SHA4285c91ffd0b45568d79472a782c8ca05a4cd475eca8cb7ce196af65d74f911b.
Captured source.json264722B has12cases/891inputs/890expected observations. Parent
verifies gzip exact equality and19oldGit/current source pins. Native daemon IPC
artifacts are engineering baseline only, never frozen expected values. New capture,
trace/live peer leaves, native CLI wrappers and actual/private outer are being
integrated. Firstbuildrepair83a6fdf82 passes4inline+6capture controls but strict
Clippy fails on multiplier/Copydrops. ce496c303 removes both and preserves exact
original input order/1000ms receive and5s transition contracts. Retry39711992268 atce496 passes builds/units but strictClippy finds one indexed
outer checker loop.601a6c9ec fixes it; retry39713055560/workflow2fd4aa00a935fb21182a1606b18bd0d4826cc069 is pending.
Featurehardwared-qa-native uses existing nativeIPC dependencies. All3
original tools remain live and pending hosted proof; five original runtime files
and shared schema/import/Python bindings remain separately pending.

## Publication next

Keep source changes and `.omo` intact. Publish a coherent issue-branch batch and
update draft343/#296; final complete exact-source required/workspace/ARM results
remain necessary. Do not stop at queued CI or a PR, merge protected branches or
claim device acceptance. Latest #296 checkpoint6330165232 and draft343 describe publishedd610, four Kalman
closures, corrected Panda, AMD reproduction and pending current complete gates.


## Current USB follow-up failure and exact repair

39708204175 at5b5 passes all six actual-root USB scenarios, unchanged complete
original input receipts and native exit0. Outer then fails summary-vs-full-row
comparison; private root not reached. Explicit USB three-field exact summaries
preserve immutable original CLI shape; complete reports and packet/cleanup/seal
assertions remain. Original-source regression and full USB retry pending; no
source acceptance or #480 closure. Parent selected25 members/CRC, not full ZIP.


## Latest native platform candidates

Hardwared predelete39715118940/fd978 succeeds completely: parent full220CRC/
300618B artifact12398269946 audit, both12/891/890trace roots+two live roots,
runner12, CLI12, reappearance6, finalELFreceipt equality. Actual3tools are now
deleted and maintained platform native gate/policy533guards prepared. Counts
remain262 until complete postdelete platform+policy proof; preserve all manager/
modem/#398 checks. Parent caught and corrected scratch policy selectors so
original Python comparisons AND new native Hardwared step stay mandatory.

Modem fresh original39715283281/ca0 succeeds: full44CRC29019B artifact12399150770,
13freshsourceoutputs equalold,26pairedplans/40ops,10source+pyserial3.5provider+
ELFbeforeafter proof. Derived3mode helper reference replaces one embeddedPython
caller; full#398test and historicalbaselinefixturecopy proof pending, no observed
embedded-site count reduction accepted yet. Modemleaf+outer64e2 exists; initial
39715658566compile35errors, agent repairs missingroot/OSargv/Seek/API closure.
All6originalmodemruntime/tool sources remain live; private6absence is candidate
proof only. Nextacceptance must state5devtools+runtime1separately ifall6retire.

ExactfullFast routing39714146176/422 passes; #478 awaiting fullpublishedFast.
Currentissuebranchremote is faeef14da; newer local commits need coherentpush.
#476singlecall39712961193 is realcomparison with56half differences; instrumented
prehalf observations INVALID. Parent572CRC55MB audit. GDB39714658646 fails before
inferior; env-isolated hardware-watch retry8c34aec is pending. No production
numerical fix. USBtwo-root39713225978/faeef runs sixcases/root; finalproof pending.

## Active coordinated validation after faeef

- USB39713225978/faeef completed all six actual cases and exact original source
  identity receipts, then timed out in an existing-output rejection control:
  large input hashing preceded output admission.8a97671a8 now rejects foreign
  files/directories/dangling links before hashing; authoritative create_dir still
  owns final publication. USB-only39717263745 repeats actual/private roots and
  all controls. Full model-family acceptance and#479 remain separate.
- Modem39717257885/8a976 fails compilation:24missing assertion-helper errors and
  two type mismatches. Parent/worker inspected the complete three-member artifact
 12399378285/SHAa08bc9c84e9cef8309f21fdb76d8285c5673449a57c735f070ee814763edec42.
  e481a530c/9afb381a8 repair all reported errors; retry39718465632 is pending.
  All six original runtime/tool sources remain live and unaccepted.
- #481 is a QA-only /proc FD enumeration race, with explicit vanished-FD records
  and fatal non-ENOENT/directory-loss behavior. f0d81af61 adds deterministic
  controls;39717945321 runs them plus the complete maintained native ARM job.
  No radar detection/lead algorithm or NAS replay source changes are involved.
- Hardware-watch39716179228 had four explicit ineligible-host skips and no
  numerical observations.39718507661 samples eight naturally allocated hosts,
  maxparallel4, with the same exact host/ISA/provider/full-output guards. A skip
  is not numerical success and no production CPU target is forced.
- Full original model pipeline39713242787/job119609922762 fails on road model
 1344-dual frameindex0/frame_id3: of10304rawbytes onlybyte399 differs, float99
  original-0.0/native+0.0. Both input-image hashes match frozen manifests. CPU is
  EPYC7763 with the recorded LLVM18 provider. This is distinct from the2212B
  driver/#476 observation; common causality is unproved. Preserve exact bytes.

## Hardwared3 accepted after pinned maintained postdelete proof

39717847363/job119624142986 atfc0f017bd96419c81af878ef211ed15152fc2eda,
workflowc6929deae721b30e7eea0478dea78fc94b31685e, passes the complete platform job
with global Rust/Cargo1.94.0. Both actual/private roots require3absent and retain
12cases/891inputs/890observations plus live IPC,12CLI admissions, configured
runner and6reappearance controls. Parent independently checks all4165CRCs in
artifact12399722817/1280882B/SHA41fcea8bc33b6cc454b3926228ece51989c0b5baaf647c5cbf7ada03055020d4,
16Git source pins, byte-identical held-before/after and all4full CLI result pairs,
plus exact engaged/offroad kmsg bytes. Independent worker corroborates5ELF seals
and64trace files/35successful execs/Python0. Original manager/modem14scenarios,
#398old failing EOF and current15passing helpers remain intact. Reuse successful
unchanged full533guard policy39716711789; accept precisely3baseline tools.
Modem embedded reference was absent from the earlier execution-edge inventory;
add one explicitly closed row rather than incorrectly decrementing37pending.

Current published4d599 full checks are Fast39719156487, Integration39719156492,
Rust39719156536 and userdocs39719156450/39719250498. #481 is now closed after
parent full artifact/source/roster audit; its Python QA remains pending conversion.
Modem39719117814 reaches units but fails three capture reads with EOF after
held-file hashing. Repair must seek/read through the held admission helper.
Retained Python manager launcher imports modem.py: current closure is5devtools,
not runtime1, despite the native Rust catalog mapping. Add explicit original6,
actual5retired/runtimepinned and private6absent states rather than weaken absence.

USB-only39717263745/8a976 passes both roots×6 plus24reappearance controls and
retained CLI/policy/foreign controls. Parent audits full185140936B/all1401CRCs
artifact12400553554/SHAed59d34104fd76f0b8174030e702a090f69ef5b41a26f71563065ed4e7b64325,
full JSONL summary/report equivalence and exact source/owned beforeafter receipts.
Full grouped proof remains pending (#480/#479 and six counts unchanged).
Current4d599 complete Fast39719156487/39719148455 and Integration39719156492
pass;#478closedagain. Rust39719156536 navigation119628631058 repeats#457before
SIGINT at unchanged10sbarrier; inspectnewretainedphase/procdiagnostics.
Watch39719769987greenisall8explicitwrong-hostskips, notnumericalevidence.
Compatible-host unchanged-binary2samplediagnostic retainsstrictrequiredISA and
fullcapturedbase/debug-outputcorrespondence; it cannotestablishsame9V45behavior.
