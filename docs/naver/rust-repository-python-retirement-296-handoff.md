# #296 current implementation handoff

This is the current execution state, not a completion report. Historical exact
runs, failures and artifact identities remain in the
[evidence ledger](rust-repository-python-retirement-296-evidence.md) and focused
family records. Git retains earlier handoff versions.

## Scope and execution boundaries

- Worktree: `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`.
  Branch `codex/feat-296-repository-python-retirement`, draft PR343→dev.
- Preserve dev-md/#338, the primary checkout and `.omo`. Do not merge the two
  tracks, change protected branches, publish a release or deploy a device here.
- Parent6.1Sol owns supervision, review, Cargo/lib/bin registration, maintained
  workflows, source deletion, documentation and Git publication. The latest user
  permits6Sol/6Luna worker selection by task complexity; preserve worker ownership.
- Local checks: syntax, scoped rustfmt/diff and stored-data inspection only.
  Functional tests, builds, Clippy, original executions and ARM run in GitHub
  Actions. Never import or execute project Python locally as a workaround.
- Reserve25GiB plus estimated growth before builds/downloads/large copies; recover
  at least35GiB if below the floor. Incremental disabled, two Cargo build jobs.
- The user has no device connected and will pull/test boot, recording and upload.
  Do not request USB or claim device/drive acceptance or vehicle CPU improvements.
- Continue all maintained Python conversion, including compilers, development,
  tests, vendor code, notebooks and embedded execution. A Rust launcher invoking
  Python does not retire the underlying Python functionality.

## Accepted source retirement and open global gates

Inventory:242replaced-local/3partial/3335pending of3580 baseline entries. Additional
helpers/notebooks and observed execution edges are tracked separately. The latter
has53pending sites before the new14-caller metadata integration is accepted.

Accepted focused families include outer Panda39625704015, device39640526320,
protocol/CAN-I/O39647358174, and actual four-source Driving/Desire39663027128.
The last run covers two absent roots, model6/collector6/fault2 per root, eight
reappearance controls and all24,585 Desire rows/47scenarios. Retain independent
original data; never restore the four deleted shared Python helpers.

Teleop39670630661 passes34 independent computations/10 disposable Git effects;
its maintained caller policy integration passes39674181956. No real repository
tag or remote was modified by the Git fixtures.

Final current-source full PR/workspace, ARM and updated outer Panda proof remain
pending. The last full PR39617311670 at f5c8ee8d failed only on the Desire source
hash; the later focused four-source gate fixes that failure, but does not replace
new exact-source final validation. Generic ARM/QEMU outputs are not sealed AGNOS
release/device evidence. #457 startup stall and #474 PTRACE_LISTEN EIO remain
unexplained; later successful runs do not establish root causes.

## Recovery: functional success, executable provenance integration pending

Full39677229740 passes eight controllers in checkout/private roots, both ordinary
and traced,17 focused units and strict Clippy at source
`0501612741fe402bedf50ead847f2f85fc9f5587`, workflow
`638e08ed71644364841b895371b4da73c8fd2bc5`. Parent audited artifact12382569025:
68,489,895bytes,11,992CRC members, SHA256
`05c62eecbe6debdd8a4b90e57dc67a12c3bc363e581f91e2552fb187cbd3296a`;
all32 controller admissions,26 unchanged held inputs and4×52 owned alias rows.
#472 fixture sequencing/complete Git-repair snapshot defect is closed by that proof.

The13 recovery Python tools are still present; server.py/repo_update.py remain
independent live original consumers. The old `recovery_family.rs` test and Python
pair-only APIs still need removal together with maintained caller migration.

Worker owns creation receipts in eight recovery leaves and
`recovery_exec_audit.rs`/`recovery_family_exec_policy.rs`/audit CLI. A created file
cannot be labelled pre-harness sealed: native alias rows bind held source inodes;
copies/scripts get actual pre-exec creation and final held-descriptor receipts.
The trace parser holds complete PID/file/SHA rosters, rejects Python/unknown or
truncated execs, and preserves all raw argv. The first complete contract is closed
executable provenance, explicitly **not exact argv parity**. Bound the ephemeral
process-child JSON argument; all resulting child executables still need admission.
Do not modify the shared production launcher merely for this audit.

Parent registers the new policy/bin, captures provider/native seals before the
harness, integrates post-run admission, reviews the worker changes and runs the
hosted gate. Then delete the13 sources, orphan APIs/old test, migrate eight main
calls and the embedded CWP test selector to native tooling, and run the actual
post-deletion grouped gate. The stale cleanup patch at base010f521 must be rebased,
never blindly applied over the new creation hooks.

## Metadata:14 maintained callers migrated, provider-order correction pending CI

Original39677845650 seals67 observations/14 actual callers. Manifest SHA256
`71d33e6e25f62c7a51240ecb4eb908acd35351a911b482869181fbfdffd130c1`.
Pure data lives at `tests/fixtures/ci-receipt-original/original` in dev-tools;
its69-file roster excludes the sibling README.

Native39679579648 at3abd3a50b passes34successful outputs/27error categories/
six explicit non-string-prefix schema rejections, four units and strict Clippy.
Parent audited artifact12383687016 (38,385bytes/78CRCs), SHA256
`6ccd1b156c2b260b5927aa24c0b99d064a01e48890436b6fc9c471011b56aeaf`.
The reader retains raw path bytes, unsorted glob semantics and4MiB bounded reads.
Four null original error summaries are resolved from sealed qualified traceback
lines; the original dataset is unchanged.

Main/template14 callers use `CI_RECEIPT_METADATA` built by native-json11 for the
host target. The missing Panda producer caught by39680000548 is fixed3851ac65d.
39680731634 passes five units/actual host producer but catches case61's different
filesystem enumeration order.9076ccc41 preserves original fixed-order selector
comparison and permits only the measured current-provider first basename in the
actual CLI comparison, with exact member roster and retained projection receipts.
Run39681730943 tests this correction using the **actual maintained host binary**,
plus workflow policy/bootstrap and strict Clippy. Inspect the result/artifact;
only after acceptance mark14 execution edges replaced (53→39pending).

## Driver/startup/USB six-source gate

Startup/USB native ports reuse strict current-catalog/CPU/LLVM admission, held
sources/assets and descendant cleanup.39674181956 accepts eight units/process
controls, strict Clippy,13disk-bootstrap and6policy tests.39675298032 separately
accepts three USB bounds/roster controls; its separate800MiB/file/1GiB asset
bound leaves ordinary512MiB catalog admission intact.

Grouped native Rust test `model_family_retirement` is registered with
`driver-daemon-native`. Preflight39680642370 at ec13cc17e passes four regressions
and strict Clippy; parent audits artifact12383198793 (5,582bytes/sixCRCs), SHA256
`1345c77d44d0c458cede76b8aa01bc227af4ec2289eb15d85288685405b46d41`.
It holds final accepted reports, compares full stdout JSONL with report rows,
keeps both roots unchanged, and reports interpreter execution as not measured.
Full8startup/6USB plus driver ordinary/raw/collector traffic and24reappearance
controls remain pending after actual six-source deletion.

Keep **separate4-model driver and5-model startup/USB data/catalog receipts**.
Only metadata PKL matches; the other DM/warp PKLs differ. Do not relabel a five-
model receipt as four-model, or replace independent model/source observations.

Original capture39680582209 at fixed0ec6cbae2 uses authenticated helpers and the
correct immutable runtime-b55552db release. Parent independently verified whole
archive1,155,620,561bytes/SHA256
`6dc8965b2423f111631b6fb6eb9dc5fde3800f46b7785595f595fe348b66d807`,
45assets+3licenses/780,342,357bytes, original e758b96d model, a25b81e9 runtime and
1bb0c80c manifest. The previously selected1f00f0b7 release has a different model;
no pin was relaxed. Never execute ARM release binaries to stage these data.

After independent capture acceptance, retain fixed data/manifests, finish native
preparation orchestration without new Python helper payloads, remove the six
sources and both maintained old Python calls, and run the combined native gate.
The existing unconverted model compiler remains explicit pending scope; invoking
it from a Rust preparation command does not count as compiler retirement.

## Publication and records

Publish a coherent issue-branch batch, update PR343 and inspect exact-source
required/full checks. Diagnostic candidate workflows execute an explicitly pinned
checkout; their unrelated Fast checks use the diagnostic tree and are not proof
for that checkout. Serialize diagnostic branch pushes.

Latest #296 checkpoint comment6327280537 links the above evidence. Keep #296 open
until full criteria are met; no source inventory, partial gate, queued CI or PR
creation is completion. Detailed functional ownership and next families are in
[the grouped ledger](rust-repository-python-retirement-296-families.md).
