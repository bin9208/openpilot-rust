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

Inventory:255replaced-local/3partial/3322pending of3580 baseline entries. Additional
helpers/notebooks and observed execution edges are tracked separately. The latter
has37pending sites after metadata14caller acceptance, recovery CWP and soundd source staging.

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

## Recovery:13-source retirement accepted

[39687004413](https://github.com/bin9208/openpilot-rust/actions/runs/39687004413)
passes at57594f054a4454d09247f88d0630c39145853dd7, workflow
ba1cfe4dd7a61f82b70988477fcd245a27faaf18. Eight controllers in two actual
13-source-absent roots, ownership regressions, strict Clippy and13disk/6workflow
policy cases pass. Eight maintained calls/CWP selector are native; obsolete
Python-only APIs/test were removed. server.py/repo_update.py remain live elsewhere.

Parent audits artifact12386833535:35,078,352B/7,346CRCs, SHA256
41891171b8c74c2c1dc9e7965e4d9645bdbb989dba55dfa4690d7ddc18da31d8.
All2,508trace files/roster/SHA and4,795exec attempts are independently counted:
2,393success/2,402explicit failures, observed Python exec0.25unchanged presealed
program pairs,96created executable bodies (86scripts/4copies/4hardlink aliases/
2tmux symlinks), two52-row owned-alias receipts and symlink creation/final rows
are independently checked. The exact executable/result contract retains raw argv
without claiming argv parity. tmux source-created symlinks hold O_PATH inodes and
bind the presealed provider; unknown exec rejection remains unchanged.

Earlier compile-tuple and missing tmux-alias admission failures are retained in
the evidence ledger. #472 functional closure stays accepted; #474 historical
PTRACE_LISTEN/EIO root cause remains unexplained. This proves no device behavior.

## Metadata:14 maintained callers migrated, provider-order correction accepted

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
Run39681730943 passes using the **actual maintained host binary**, seven units,
all67original observations,13disk/6workflow policy cases and strict Clippy.
Parent audited all83CRCs and independently recomputed every case. Artifact
12384097663 is44,650bytes, SHA256
`3e7b85fe188eeb151983fad2540b7c4cb406fb6cb6ba920642084fe7d13369ea`.
The14 metadata execution edges are accepted; recovery CWP subsequently reduces pending sites39→38. Metadata baseline file counts are unchanged.

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

Original capture39680582209 at fixed0ec6cbae2 used authenticated helpers and the
correct immutable runtime-b55552db release. Parent independently verified whole
archive1,155,620,561bytes/SHA256
`6dc8965b2423f111631b6fb6eb9dc5fde3800f46b7785595f595fe348b66d807`,
45assets+3licenses/780,342,357bytes, original e758b96d model, a25b81e9 runtime and
1bb0c80c manifest. The previously selected1f00f0b7 release has a different model;
no pin was relaxed. Never execute ARM release binaries to stage these data.

Parent independently audited capture artifact12383214627 (130,144,004bytes/735CRCs), all34startup and80USB sealed data files, eight/six case rows, and68byte-identical original USB observation bodies. Durable pure data is `tests/fixtures/model-family-original`; startup manifest4c54c0d6/USB8774d675. USB original controller exited0; wrapper failed only the late unused-msgq import inventory assertion after its before/after equality checks. Data-only finalization explicitly retains original-wrapper-incomplete status and three SHA-bound ELF entries with unknown lengths (`bytes:null`); it never invents observations, reruns source or substitutes native output.

Native preparation aaf5efa0e provides archive staging and separate driver/family catalog bundles, current model/source/CPU/LLVM seals and atomic publication. Bounded compiler execution preserves timeout partials and states when limit-error partials are unavailable. Hosted preflight39685067872 passes four new preparation and four grouped support regressions/compilation/strict Clippy; parent audited artifact12385584115 (6443B/eightCRCs/SHA26e41bc9d39505830a7bf10ea936a893c3781d1b41465eda1d0d656e21c8ab96). Full39686001854 atcb9f508e0 passes separate4/5-model catalog exports and genuine45-asset/three-license USB staging, then catches Git CRLF normalization of a sealed installed marker (879→849B).1ccf8437c sets byte-preserving attributes and restores original bytes without changing either manifest; all114sealed staged Git blobs match original pins. Retry39689390187 source1ccf8437c/workflowba2ca41f4 is pending actual traffic acceptance. Current candidate removes the actual six Python sources and two old workflow calls, adds required `model-family-retirement` to the aggregate gate, and binds explicit atomic bundle paths. The independent review caught and excluded USB cache model hardlinks from artifact upload (about18.6GB of duplicate input). Acceptance remains pending the full grouped gate; six-source counts are unchanged. Native soundd source staging7219cf5e7 is accepted39688832018: five controls/strict Clippy, actual original-vs-native CLI20file comparisons/173533B, seven audio modes in both roots and22closed native execs/Python0 per runtime trace lane. Parent audits artifact12388550588 (20121548B/462CRCs/SHAf34db8b407a66daa9140cd8efde7ac37300f7de09c711d1e971fd571de751586), exact original heredoc SHA56630b535e2da704cf44ce523f9fbcfe662f9e32d2c42821a6938f00a500bf77,21pins/two native stage receipts and both traces/provenance. No receipt enters the20-file root; other audio Python remains pending. fbb6a007f adds an early frozen-byte regression and failure-tail diagnostics to the model gate; its new regression awaits final hosted execution.
The existing unconverted model compiler remains explicit pending scope; invoking
it from a Rust preparation command does not count as compiler retirement.

## Publication and records

Publish a coherent issue-branch batch, update PR343 and inspect exact-source
required/full checks. Diagnostic candidate workflows execute an explicitly pinned
checkout; their unrelated Fast checks use the diagnostic tree and are not proof
for that checkout. Serialize diagnostic branch pushes.

Latest #296 checkpoint comment6327837803 links the above evidence. Keep #296 open
until full criteria are met; no source inventory, partial gate, queued CI or PR
creation is completion. Detailed functional ownership and next families are in
[the grouped ledger](rust-repository-python-retirement-296-families.md).
