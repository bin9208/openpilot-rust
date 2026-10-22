# #296 active implementation handoff

Work remains unfinished. The [evidence ledger](rust-repository-python-retirement-296-evidence.md)
retains historical accepted/failed sources, workflow IDs and artifact SHA/CRC
records; Git retains earlier handoffs. Do not restart accepted unchanged groups.

## Scope and isolation

- ONLY edit `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`,
  branch `codex/feat-296-repository-python-retirement`, draft PR343→dev.
  Primary `/home/fma1/rust/openpilot-rust` belongs to dev-md/#338; preserve it
  and untracked `.omo/`. Only remote bin9208/openpilot-rust is authorized.
- Entire remaining Python development/build/test/vendor/notebook/embedded scope
  stays active. A Rust wrapper around Python does not retire its functionality.
- Accepted baseline: **274 replaced-local / 3 partially-replaced / 3303 pending
  of3580**. Routing TSV covers3306pending/partial entries. Additional observed
  embedded edges: **67total /39pending /25replaced-local /2non-executing-text /
  1native-existing**. The two hosted exporter tests are explicit temporary
  pending Python edges, without baseline credit.
- Local syntax/scoped rustfmt/TOML/YAML/Bash/diff, Git/API and inert saved-data
  inspection only. No local Cargo/build/test/Clippy/project Python/model/provider
  execution or dependency installation. Hosted Actions performs functional work.
  Rustfmt: edition2021, skip_children=true. Keep25GiB+growth; currently80GiB free.
- User has no comma device and owns final boot, recording, upload and driving.
  Do not request USB, deploy/reboot, claim device acceptance, merge protected
  branches or publish a release. Parent owns serial commits/publication/review.

## Source and full checks

Implementation source: c12f3d1191dbec9694478a3bed0b04bf2edf4e4e.
Issue-branch remote/PR343: f09eee7b8d352cc3b4463acf5f467a786f0d310d.
Newest diagnostic branch: 8bd27349a8955d95e83db4bda99060f65c785f72.
Do not lose newer commits or mix unrelated route WIP into a source pin.

Published PR synthetic merge48abb4d87797d84c03be1595e9122c4b1e023e2d:
Integration39754486834 passes all16jobs; both Fast39754486617/39754477763
and docs39754486636/39754745503 pass. Full Rust39754486991 currently has45
successful jobs, two model failures and one running workspace119742946369.
Generic ARM119742946791 succeeds. Workspace timeout210min
is intentional, so avoid cancelling/repeating it merely for a small push.
Successful LLVM20/UI gates are included in45. Generic ARM is not AGNOS.

The old full-model failures are distinct: original pipeline119742946401 reaches
actual raw-road prediction drift without helper; family119742946604 stops
before numerical traffic at the historical schema.py source digest. #474's
historical PTRACE_LISTEN/EIO remains unexplained. #484 is closed after its
complete affected recovery gate passes: artifact12421257859/34970712B/SHA445f5f07,
all7340CRCs/2508trace seals parent-audited.8controllers×2roots,5271closedexecs,
Python0; pending-tunnel SIGTERM exits182.981276/193.046875ms within unchanged3s.
This does not close #474 or imply the complete workspace has finished.

## Accepted results to reuse

- Complete generator/manager postdelete39746277084/sourceac721024: platform
  12412880586 SHAee4fcf95/all8426CRCs and policy12412410021 SHAe1159aa6/all4CRCs.
  Manager62native execs/78traces and generator12native execs, seven original JSON
  bodies, strict checks and actual platform composition pass. Generator credits
  entry274; runtime manager/Params/swaglog Python remain live.
- #485 output ownership RED39749673407 then GREEN39752278749/source06956b79:
  31distinct tests, both bins, strict Clippy/no-default admission. Held ownership
  prevents deletion of foreign replacements, with non-atomic pathname limit
  explicit. Issue closed after published acceptance; not full-model acceptance.
- #457 closed after measured dev/test num-bigint-dig optimization and published
  complete navigation119742946180/Integration acceptance. Actual interrupt476ms,
  fresh RSA234.015199ms, signal2/reaped probe; all8lifecycle and complete original
  comparisons pass. Artifact12416295492 SHA418df410/all1430CRCs audited. RSA2048,
  release behavior and10s deadline preserved; no universal historical-cause or
  vehicle CPU claim.
- Native guarded-add lean39755306372/source06956b79 reproduces entire accepted
  first-context15800Bobject/optimizedIR under recorded-target nonexecuting LLVM18.
  Raw no-EOF source remains unchanged; explicit LF boundary is separately equal.
  Artifact12416082247 SHA12e27ef3/all30CRCs. Twelve historical exceptional half
  patterns/eight buffer poststates are reused honestly, not reexecuted. #482 open.
- Driver39741736692 identifies actual term2 separate-add/FMA drift. Nonexecuting
  captured-target39752466938 reproduces full failing11640Bobject and post-O2IR:
  contraction occurs later in backend codegen. No driver compiler fix accepted;
  #476 open. No per-index masks, changed goldens/tolerances or CPU forcing.
- Source-present route composition: manager39753960752 semantic comparison passes,
  then old exact-argv audit fails (runtime never ran). Corrected saved trace has
  68calls/66success+2exact df ENOENT; only source-authored Git argv added.
  Runtime-only39756811673/source192ff501 passes33held seals,12exact provider links,
  six native execs, real GET/PUT and each complete qlog transfer. Artifact
  12416731303 SHA29709384/all71CRCs audited. Both Python controllers remain live.
- #490 actual publisher RED/GREEN39761141735/sourcee0805edb succeeds: zero-rewrite
  helper policy collision reproduced under old publisher; exact bidirectional
  file/directory roster rejects stale files/dirs/aliases while retaining exact
  same-policy reuse and all old identities/bytes/pointer. Evidence12418930119
  SHA0ad014db/13CRCs, tests12417334939 SHA014a8ca8/17CRCs,19bindings and helper
  ELF526a524c independently audited. First39760699556 stopped before publisher
  execution because depth1 omitted old tree; R2 uses bounded3commit checkout and
  reuses exact-source successful Clippy. Fix committed; close#490 after next
  issue-branch publication. This is not exporter retirement/full-model acceptance.

## Active model closure

Actual exporter/native-helper bridge R2 run39756443777/source6e54a58b passes
its bridge test and both-resolution pipeline/publication/input comparisons.
All2914sidecars across10catalogs match; family1454sidecars across6catalogs match.
Later pipeline failure is the scratch composer losing workflow-level Rust1.94
and invoking ambient1.99 Clippy; maintained workflow already pins1.94. Do not
change runtime algorithms for those new lint messages. Family source admission
fails separately before numerical traffic. No passing aggregate is claimed.

Current-source admission fix committed3fd4e7a4 keeps original model/source
manifests immutable and admits only separately sealed current model_export
Python files through authenticated current-catalog receipts. Runtime/checker
historical pins remain strict; mutation/alias/stale-receipt tests added.
Continuation39761920908/workflow738785adb/source3fd4e7a4 remaining-road
job119767923402 passes all four tests including the complete two-root retirement
gate (1896.83s). Parent and worker audit all4614CRCs in1,534,656,177B artifact
12420199425/SHA48fa9b05, both roots'6model/6collector configurations and all8
reappearance rejections. Worker audits1092sidecar hashes/20bindings. Same-source
helper/caller integration is committedbb3b4fdd3 in both maintained model jobs;
raw sidecars use only bounded checked-NUL-roster archives, with postarchive source
hash guards. No new inline Python auditor; existing75min bound is retained from
measured50m44s composition. Maintained hosted execution remains pending.
Updated family119767922950
has failed the actual first driver
collector case after source admission passes. Both explicitly select1.94 and
keep original complete numerical/absence/fault gates. New source/host catalogs
must be exported; old6e54/znver4 receipts cannot be relabeled or forced onto hosts.

The family failure reproduces #476's exact58456ffe native/99b8b2fe original
frame0 bytes:439/553float32 values differ, maximum0.01171875, while both native
paths, both1382400B warped inputs and original/native warmup agree byte for byte.
Parent independently audits162986409B artifact12420001190/SHA957da696/all1669CRCs
and those complete bodies. Worker checks1454sidecar hashes and20bindings. Later
startup/USB and second-root cases do not execute after the required driver failure.
Current exporter admission is accepted only as admission, not numerical closure.

Worker exporter_bridge_r3 owns model diagnosis/native cached-program candidate.
Nonexecuting packaging39769969526/workflowfd93a4ff/source3fd4e7a4 passes all44
complete cached image-to-ELF comparisons. Parent audits500840B artifact12421643434/
SHA10b55633/all71CRCs; worker verifies17heldinputs/51generated hashes. Generated
code is never loaded/called; no ISA/ABI/numerical acceptance follows. A native
iced-x86 predicate is scratch-only with an isolated dependency manifest, pending
hosted5negative tests/all44images. No production dependency/loader change exists.
Maintained model integrationbb3b4fdd3 awaits issue-branch publication and actual
hosted result. Retain complete driver acceptance separately; do not stop at a
diagnostic PASS or treat cached-model consumption as compiler retirement.

## Active ordinary uploader/logging group

Original39758140178/source06956b79/workflowa38acdb7 succeeds. Artifact12417680413:
506656B/SHA7be58982b6ad420fb97a2085e1e782bec5edb7ca3c77d8e2019eab731c8a899e,
all474CRCs/461seals independently audited by parent and worker. Independent
scope212filesystem/10000backoff/32HTTP/signing/18logging×2collectors is retained;
no private key files archived. Frozen reference JSON SHA b1ce58a7… and logging
cdf42bbf… stay unchanged. Raw observations remain external, not blindly in Git.

Sourceb53be530 adds both real Rust controllers, narrow features, CLI wrappers and
six tests. Preserve-order is enabled only in uploader-logging-native for original
header repr. Live original-policy collector lane is the independent Rust adapter;
historical actual Python collector evidence stays explicit. No Python process
calculates native expected values. Both old helpers still have seven/two sibling
consumers and are not deleted or credited.

First native39762635403 has zero jobs from invalid runner.temp job-env context;
R2 initializes proof path through GITHUB_ENV. R2 run39762866909/sourceb53 builds
both candidates, then fails only Clippy needless_range_loop before functional
traffic. Fixb897edcf iterates TOPICS directly. Failed artifact12419380772:
526759B/SHAa07b6b2d87c905085f67c9bf18efa59d1c739e8f7fb6ebe6601b250464e3523d/485CRCs
worker-audited. R3 run39763672720/workflowce5f93658/sourceb897 passes compile,
strictClippy, no-default admission and test compilation, then fails policy5's
files/events order. Last/results/next and all other fields agree. Sourcee0acfdbb
uses probed Linux tmpfs to reproduce captured same-priority input relations,
preserving cross-priority input and frozen expectations.

R4 run39766039639/workflow8710ba37/sourcee0acfdbb passes all212 full filesystem
outputs and their live xattr checks, then HTTP0–12. Parent independently checks
artifact12419949779/3106694B/SHA3b6740df/all1186CRCs, all212 complete saved output
objects and496captured tie relations.74fixtures retain unsorted priority inputs.
HTTP13 redirect301 fails: the diagnostic peer rejects the provider's empty
chunked redirected PUT. Backoff/logging have not run. Source67f099bf adds bounded
chunk framing and flushes HTTP observations before comparisons, leaving daemon
and oracle unchanged. R5 run39767931037/workflow728f3586/source67f099bf uses recipe
SHAd9f7c6a50a8b109d9ffbe442cba5998e8f4a571b90bfc33823395a84d276d099;
ordinary scope passes3tests/all212filesystem/10000backoff/32HTTP cases. Parent
audits artifact12421835922/3242922B/SHA536ead05/all1308CRCs, all244complete outputs
and10000backoff values. Logging separately fails mismatched new peer queue size;
source40f37bea3 restores pinned250KiB deviceState and adds two actual-TCP framing
tests. R6 run39770418874/workflow276c346d8 succeeds: all18cases×2collectors,
74disk/10error records,3logging tests and2framing tests. Parent checks6166637B
artifact12422391683/SHA23ac056a/all1084CRCs and all36complete comparison rows.
Ordinary traffic runs0times in R6; its sealed source/provider/feature closure is
explicitly reused, without emitted-binary equality claims.

Exact independent observations and original capture/report are committedffefb1539
under tests/fixtures/uploader-original-group, with provenance manifest. All four
Git blobs retain original bytes/SHA. This does not retire either Python helper.
Sibling capture39770735490/workflow72bfd4d8b/source67f099 succeeds for new daemon,
interruption,timeout/log-fault/typed-Params scopes. Parent verifies artifact
12422761878:543998B/SHA69f9c16983a0699d7478455016b94dade68374e346b72a12a1e669354ad8abee/
all886CRCs and original-only observationSHA30f9e1f2,23pins,10timeout/30fault/14Params
rows; worker checks866seals. Native sibling WIP is registered but uncommitted.
Parent review finds unauthenticated same-name namespace removal and pathname
permission restoration; #492 tracks correction/regressions before first commit.
Preserve the model-runtime WIP and .omo when staging. Parent-owned Cargo/lib
registration is applied, has no new dependency, and must accompany sibling code.

Source7689484bb switches three accepted maintained uploader calls (support
ordinary/logging and startup signing) to actual Rust CLIs with exact independent
fixture SHA, explicit root and selected locked build plus capacity guard. Existing
timeout/fault sibling calls remain Python. Existing isolation-policy tests now
require native calls. Worker read-only review has no blocker; maintained hosted
execution is still pending publication.
Worker guarded_proof_r3 owns actual compile/Clippy/functional failure repair and
then the remaining daemon/interrupted/timeouts/log_fault consumer closure.
Do not repeat accepted ordinary source traffic merely to capture new siblings.

## Route ownership candidate, issue#491

Routeworker route_upload_r3 owns the lifecycle/signal/root/CLI closure and hosted
proof. Parent reviewed and committed all15files at8444df20b94f83cec92dd7e3f6a74887022c3604;
uploader registrations are preserved. Original upload controllers and shared
helper stay unchanged. This source is a candidate, not retirement credit.

Read-only review found restarted socket inodes skipped permanently, actual statsd
bind able to unlink foreign global/tmp/stats after precheck, and bind-then-exit
missed by live-FD adoption; acknowledgement-based fixtures hid that race. Worker
replaces those global paths with a preowned private QA IPC root across
logging producer, collector, statsd and example, preserving normal defaults.
Require focused default/rejection tests, real provider restart/foreign-path and
unacknowledged early-exit proof; no sleeping fixture alone proves provider safety.
Dedicated CLI subreaper retains WNOWAIT/pidfd identity until final signals/reaps.
If complete owned-child cleanup cannot be confirmed, its guard preserves every
resource. R4 run39767373708/workflow83356d0d/source8444df20 has eight hash-checked
external payloads; recipe SHA89bdde5082506ef76f8cffe7a4d78b6bb86d064627fb75b7cc6f7dd44f2efab9.
Focused strictClippy,3controller units,4provider endpoint tests,10CLI/semantics
and7default lifecycle tests pass. Two of three actual-provider tests pass;
actual-manager cancellation fails at missing acknowledged logmessage ownership.
Source3f60fdc35 fixes the readiness predicate without dropping the assertion.
R5/39769134663 and R6/39770477410 pass ALL focused gates, including3actual-provider
tests. R5 private checkout fails LFS smudge before traffic; R6 skip-smudge matches
primary tracked pointers and full tracked-byte equality, then original help emits
pycapnp's inherited-PWD mismatch warning. Strict help stderr checks stay intact.
R7/39772010352/workflow9eedccf30/source3f60 passes all focused gates,44CLI cases
and complete source-present manager/runtime parity plus exec audits in BOTH roots.
First source-absent manager records/uploads, but collector socket ownership is
not acknowledged. Accepted sockets inherit the listener pathname: the observer's
all-row uniqueness is a concrete defect, not a recovered failing /proc snapshot.
Sourcec12f3d119 selects a unique stream listening inode before descendant-FD
authentication; a real connected-peer/ambiguous-replacement regression is added.
Four QA validators also reject symlink roots with trailing / or /., retaining
valid real-directory spellings/defaults/explicit collector override.
R8/39775898808/workflowe515327503a2cd7c4bcf04d5931213003df4a713 is running five
expected-assertion RED cases, focused GREEN and fresh complete two-root proof.
Pinned recipeSHAdcab047f07ad1f468a4ce70d04f259e07069942f9b671abadf68035cef900983.
Review repaired actual results, raw
traces/bodies/seals before deleting originals or crediting them.

## Publication and next steps

Parent alone serially runs scratch publish-single.py; always drain its shell
session before a new push. Latest publisher is drained. GitHub connector API works
while local api.github.com DNS has failed; github.com Git push works. Download
artifacts through connector temporary links without printing signed URLs/data.
All scratch recipes/evidence live under
`.analysis/scratch/2026-10-22-family-execution-296/`.

Native ISA run39775186997/workflow8ed5e42d3 stops at compile: forwarding literal
macro tokens hides is_x86_feature_detected named-feature matches. No ISA gate ran.
Worker replaces forwarding with explicit detector calls, preserving the mapping.
Repaired recipeSHA7e4499317cdfe1053d7a7d6c9bc2440632e147f1334f6daba523670eec37aa7c
at8bd27349a/sourcebb3b4fdd3 passes39776104353: all5negative units/all44images,
58028decoded instructions. Artifact12424538606:2062901B/SHA3a3c108b/all41CRCs,
all44original source/image identities/text extents and complete native receipt
parent+worker audited. Generated image execution/ABI/numerics are explicitly false.
Worker owns native container/loader/exporter integration and actual dependency
vendor manifests. Parent review additionally requires exact linked-library cached
symbol roster, not only exact container count. Runtime candidate remains
uncommitted/default-disabled; no driver numerical acceptance or retirement.

Continue model compiler diagnosis, native uploader consumers and route R8; repair actual failures without
altering original evidence. Complete route ownership fix/proof, then grouped
consumer conversion and native caller transitions. Push a coherent reviewed
batch to the issue branch, refresh PR343/#296 and close only accepted defects.
Follow exact-source required full gates; keep user device acceptance separate.
Do not finish the task because a PR exists or CI is merely queued.
