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
  Rustfmt: edition2021, skip_children=true. Keep25GiB+growth; currently82GiB free.
- User has no comma device and owns final boot, recording, upload and driving.
  Do not request USB, deploy/reboot, claim device acceptance, merge protected
  branches or publish a release. Parent owns serial commits/publication/review.

## Source and full checks

Local committed HEAD: b897edcf68374a1ce563d4007df357ebb1e8a99a.
Issue-branch remote/PR343: f09eee7b8d352cc3b4463acf5f467a786f0d310d.
Newest diagnostic branch: ce5f93658ba7d5511a276441d838a749049502f8.
Do not lose newer commits or mix unrelated route WIP into a source pin.

Published PR synthetic merge48abb4d87797d84c03be1595e9122c4b1e023e2d:
Integration39754486834 passes all16jobs; both Fast39754486617/39754477763
and docs39754486636/39754745503 pass. Full Rust39754486991 currently has44
successful jobs, two model failures and two running jobs. Remaining workspace
119742946369 and generic ARM119742946791 must finish; workspace timeout210min
is intentional, so avoid cancelling/repeating it merely for a small push.
Successful LLVM20/UI gates are now included in44. Generic ARM is not AGNOS.

The old full-model failures are distinct: original pipeline119742946401 reaches
actual raw-road prediction drift without helper; family119742946604 stops
before numerical traffic at the historical schema.py source digest. #474's
historical PTRACE_LISTEN/EIO remains unexplained; #484's complete-PID fix still
needs its affected complete workspace recovery gate, despite focused success.

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
Continuation39761920908/workflow738785adb/source3fd4e7a4 runs remaining-road
job119767923402 and updated family119767922950. Both explicitly select1.94 and
keep original complete numerical/absence/fault gates. New source/host catalogs
must be exported; old6e54/znver4 receipts cannot be relabeled or forced onto hosts.

Worker exporter_bridge_r3 owns model diagnosis/caller continuation. If full road
passes, integrate same-source helper build/explicit absolute ELF+SHA and source
closure into maintained model jobs; current PR default still disables helper.
Retain complete driver acceptance separately. Do not stop at a diagnostic PASS.

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
worker-audited. R3 run39763672720/workflowce5f93658/sourceb897 uses recipe SHA
fb5596f2e0045000f4af1cb7247d2616e203d5e61055d6d83406abced8d86d58.
Worker guarded_proof_r3 owns actual compile/Clippy/functional failure repair and
then the remaining daemon/interrupted/timeouts/log_fault consumer closure.
Do not repeat accepted ordinary source traffic merely to capture new siblings.

## Route ownership candidate, issue#491

Routeworker route_upload_r3 owns uncommitted lifecycle/signal/root/CLI fixes,
fixture/tests and the installed manager example. Parent applied its v2 namespace
patch; further private IPC changes are WIP. Cargo route-fixture bin/test additions
are deliberately unstaged; uploader registrations are already committed. Preserve
both when staging. Original upload controllers and shared helper stay unchanged.

Read-only review found restarted socket inodes skipped permanently, actual statsd
bind able to unlink foreign global/tmp/stats after precheck, and bind-then-exit
missed by live-FD adoption; acknowledgement-based fixtures hid that race. Worker
is replacing those global paths with a preowned private QA IPC root across
logging producer, collector, statsd and example, preserving normal defaults.
Require focused default/rejection tests, real provider restart/foreign-path and
unacknowledged early-exit proof; no sleeping fixture alone proves provider safety.
Dedicated CLI subreaper and WNOWAIT/pidfd ordering look sound statically, but need
hosted checks. Run focused then full two-root original/native/source-absence/CLI
proof, review raw traces/bodies/seals before deleting originals or crediting them.

## Publication and next steps

Parent alone serially runs scratch publish-single.py; always drain its shell
session before a new push. Latest publisher is drained. GitHub connector API works
while local api.github.com DNS has failed; github.com Git push works. Download
artifacts through connector temporary links without printing signed URLs/data.
All scratch recipes/evidence live under
`.analysis/scratch/2026-10-22-family-execution-296/`.

Continue the two model jobs and uploader R3; repair actual failures without
altering original evidence. Complete route ownership fix/proof, then grouped
consumer conversion and native caller transitions. Push a coherent reviewed
batch to the issue branch, refresh PR343/#296 and close only accepted defects.
Follow exact-source required full gates; keep user device acceptance separate.
Do not finish the task because a PR exists or CI is merely queued.
