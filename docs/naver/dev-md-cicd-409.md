# Sequential dev-md CI/CD (#409)

Issue: https://github.com/bin9208/openpilot-rust/issues/409
Branch: `codex/feat-cicd-optimization`; PR target: `dev-md` only.
User-approved design: 2026-10-20 KST. Superpowers is not used.

## Execution contract

`.github/workflows/ci.yml` is the single validation entry point. Branch pushes
run fast checks unless an open PR to dev/dev-md/main already owns validation.
PRs and protected-branch pushes run the complete chain without path filters:

fast checks → mapped user docs and publication-policy regressions → Git update
lifecycle → legacy build/regressions → web controls → Jetlink CPU inference →
Android checks/APK → native Jetlink power/USB policy → integration gate → the existing 38 Rust runtime groups in
the former rust checks dependency-array order → rust checks → generic ARM64
build. Every architecture matrix has max-parallel 1 and fail-fast true. Existing runners, native
providers, build flags and source-comparison scope are retained.

Failure stops downstream workloads. Integration/Rust/ARM admission gates run
also after upstream failure and reject failed, cancelled, skipped or absent
results. Required names remain fast checks, check mapped user docs, integration
gate, rust checks and rust aarch64 build. The latter rejects every prerequisite
after the actual ARM workload. The expensive ARM job uses ordinary success
dependencies; it cannot start after a cancelled/failed Rust stage. The final
always-running summary rejects a skipped/cancelled ARM workload. A newer revision cancels
the old run for the same PR; separate PRs are not globally serialized.

Dedicated Naver CI/APK workflows and fast Naver-only regressions are removed.
Product sources and mixed runtime/navigation comparisons are retained. Shared
upstream guards now live under tools/ci/tests; inherited remote publishers stay
repository-guarded and disabled as before.

## Runtime delivery

Only a successful dev-md push pipeline whose merge commit contains
`[runtime-package]` requests delivery. The request verifies an associated
merged PR with that exact merge SHA and dispatches dev-md-runtime.yml with
source_sha and ci_run_id. No runtime is requested by this infrastructure PR.

The separate delivery run waits briefly for the requesting CI run to complete,
then verifies its repository, workflow path, branch, push event, latest run and
attempt, exact SHA, five successful required jobs, current dev-md head, merge
identity and marker. This avoids waiting for completion of its own workflow.
It checks out that source, builds/seals and verifies offline installation,
then publishes and verifies online installation. The native publisher rechecks
the latest completed exact-source CI and the selected branch's protection.
No fix-402 branch dependency or pre-merge artifact-SHA equality is required.

## Branch policy and validation ledger

Both dev and dev-md must require PRs and all five strict required checks,
including administrators, with zero mandatory approving reviewers, conversation
resolution, no bypass actors, no force pushes and no deletion. dev source and
its separate development track remain unchanged.

Local checks: YAML/Python/JavaScript syntax, Rust formatting and diff only.
Functional policies, failure fixtures, runtime builds and source comparisons
run in GitHub Actions. Required evidence before completion: PR run, branch
protection API readback, merge SHA and post-merge run, and no overlapping
validation jobs. Do not substitute queueing, a skipped job or a host build for
successful CI, AGNOS installation or user device acceptance.

Initial source: `d4d1c4e6`. [Branch run 39422917555](https://github.com/bin9208/openpilot-rust/actions/runs/39422917555)
confirmed branch-only routing and passed all 46 unittest policy/source guards,
but failed the disk-guard provider-argument hash because it encoded the former
YAML job order. Diff review verified the same commands, counts and argument
multisets before/after; canonical sorted hashes preserve those exact values
while allowing the approved job reorder. No disk guard was removed or relaxed.

[Branch run 39423177019](https://github.com/bin9208/openpilot-rust/actions/runs/39423177019)
on `cae7ee8f` passed fast checks (46 policy/source unittests, 13 disk-guard
regressions, Wiki, Jetlink Python and Android core/runtime tests). No deep job
ran on this branch push. Legacy planner/Card/Xiaoge/UI workflow-inspection tests
are also redirected to ci.yml and the structured result gate.

PR: https://github.com/bin9208/openpilot-rust/pull/410
Initial full PR run: https://github.com/bin9208/openpilot-rust/actions/runs/39423610369
Fast checks, mapped docs/native publication-policy tests and Git lifecycle passed
sequentially before the legacy build. Inspection found that GitHub emits skipped
jobs even for fast-only branch runs; their names now differ from required deep
contexts so skipped branch jobs cannot satisfy an unfinished PR gate.

Branch protection applied/read back on 2026-10-20 KST: dev/dev-md both require
PRs, strict five checks, administrators, conversation resolution, zero mandatory
approving reviews, no bypass/force-push/deletion. The existing dev rule was
retained. The personal-repository API rejects organization-only empty bypass
lists; those unsupported fields are omitted and readback confirms no allowances.
Exact readback and continuing results: [issue evidence](https://github.com/bin9208/openpilot-rust/issues/409#issuecomment-6291019395).

Status: full PR and post-merge acceptance pending. No runtime was delivered.

## Android prerequisite discovered in full CI

On source `481aadd6`, full legacy build/native models/camera/runtime regressions,
web controls and Jetlink CPU inference passed. Android app lint/APK compilation
failed because the existing self-hosted job had no SDK location; Rust and ARM
workloads were skipped and required aggregators failed as designed.
[Failure job](https://github.com/bin9208/openpilot-rust/actions/runs/39424631693/job/118639684171).

The fix reserves 25 GiB plus 4 GiB growth, selects a job-owned SDK, uses the
existing pinned setup action and installs the app's SDK 35/build-tools 35.0.0.
No test or product behavior is removed. Actual Android and remaining full CI
validation is pending on the updated source.

## Rust service PATH discovered after integration

On `3e136e97`, Android (including actual APK verification), native Jetlink and
integration gate passed. Workspace then failed at its first rustup invocation
after setup-python, although native Jetlink had just used the provisioned Rust
installation successfully on the same runner.
[Workspace evidence](https://github.com/bin9208/openpilot-rust/actions/runs/39427008481/job/118649749196).
Rust-consuming jobs now register the existing tool directory with GITHUB_PATH
before setup actions. This does not install a different compiler or change
source behavior. Actual continuation remains required.

Further diagnosis corrected the initial PATH-only hypothesis: native Jetlink's
post-action logged `Cleaning cargo/bin`; the exact rust-cache revision
`6323deb102c322ba6fcbdcafc7e3dddab59af2b6` deletes pre-existing regular bin files
in cleanBin, including Rustup hardlinks on persistent runners. The next run's
preflight confirmed the shims were absent, so path registration alone cannot
restore them. Native caching now excludes bin cleanup/restoration and uses a
new prefix. The common action restores absent shims from official Rustup 1.29.1
archives with pinned x86_64/aarch64 SHA-256 values, admitting disk capacity first.
It selects no default compiler; existing jobs still explicitly install 1.94.0
or their original nightly. Cloud fixtures reject low space and corrupt binaries.

Upstream evidence: [cleanup source](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/src/cleanup.ts#L82).
Rustup version/checksums were read from static.rust-lang.org release-stable.toml
and the two architecture-specific archive/1.29.1 rustup-init.sha256 files.

## Planner repair prerequisite (#413)

On `f077eb35`, cache preservation worked: workspace passed Rust tool setup,
formatting, packaging policies, Clippy and fixture generation, then the full
workspace test failed at `native_parameters` / `repair 75`. This is a real lost
update: queued repairs for a previous invalid value can overwrite a newer valid
external write. The original deadline/test is retained.
[Failure](https://github.com/bin9208/openpilot-rust/actions/runs/39430751347/job/118665978425).

The prerequisite fix adds Params::put_if_unchanged, comparing the optional raw
snapshot and committing the atomic rename under the existing Params lock. The
planner captures that snapshot in its bounded background queue. Existing put,
lower-bound/default values, nonblocking control-cycle behavior, queue size and
error handling remain unchanged. Focused stale-write, missing/empty and
concurrent-writer tests run in Actions before the full workspace stage. All Rust
executables remain built from the exact release SHA; external model/provider
producers are unchanged by this Rust-only repair fix. No runtime is published
for this infrastructure PR. Issue: https://github.com/bin9208/openpilot-rust/issues/413

### DM replay comparison input ordering (#414)

On `ec805b27`, [run 39438516023](https://github.com/bin9208/openpilot-rust/actions/runs/39438516023)
passed the full workspace Clippy/tests and focused Params repair regressions,
then failed the continuous DM comparison at recovery frame 0 (vision factor
2.0 versus 1.0). The fixture sent replay's polled modelV2 before remaining
inputs, while the oracle received the whole set. Publish modelV2 last and
cover an immediate wake-up with a cloud regression. The unchanged exact IPC
comparison now runs before costly model comparisons; no runtime DM policy or
comparison tolerance changes. Acceptance remains tracked in
https://github.com/bin9208/openpilot-rust/issues/414.

### Fresh pinned renderer staging (#417)

On `16d618aa`, [run 39447594803](https://github.com/bin9208/openpilot-rust/actions/runs/39447594803)
passed through the integration gate but failed before workspace tests:
`raylib pinned asset differs/missing; use fresh output`. The producer and pins
were unchanged; the fixed runner-temp output was rejected by the existing
integrity check. Each of the five CI preparations now allocates a fresh output
generation and exports its actual path to downstream builds/artifacts. Provider
pins/flags and all 187 disk guards remain; only output arguments change, with
the exact provider-argument hash updated after diff review. Cloud fixtures run
the preparation statements twice with owned tools to verify fresh roots and
preservation of stale files. Full validation remains pending in
https://github.com/bin9208/openpilot-rust/issues/417.

The next step in [run 39450944392](https://github.com/bin9208/openpilot-rust/actions/runs/39450944392)
passed actual fresh raylib staging, formatting, package policy and Clippy, but
then rejected the existing fixed `torque-numerics` output. This confirms the
preparation issue affects runner-temp output generally. Concrete validation
and delivery jobs now allocate an owned job directory before tools run. Shell
initialization redirects the default RUNNER_TEMP/TMPDIR while preserving
explicit subprocess test overrides and unset-variable fallback cases. Artifact
and cache actions use the matching CI_JOB_TEMP. After uploads, always-running
cleanup removes only that job directory, rejects nested/unrelated/symlink paths,
and preserves existing runner files. Provider staging/integrity checks remain
unchanged. Cloud fixtures cover repeated jobs, shell/action path agreement,
overrides and cleanup boundaries. No workflow globally locks other PRs.

On `f162005f`, [run 39455622965](https://github.com/bin9208/openpilot-rust/actions/runs/39455622965)
passed the formerly failing numerical stage, then workspace Jetlink socket tests
rejected the long job-temp prefix (`path must be shorter than SUN_LEN`); the
first failure poisoned the test mutex and produced seven secondary failures.
This was introduced by the isolation path length, not a transport-policy change.
Use a short owned `/tmp/openpilot-ci.*` directory for RUNNER_TEMP/TMPDIR instead.
Cleanup checks the direct parent, symlinks and exact run/attempt/job owner marker.
Cloud regression binds a real Unix socket with an intentionally long original
runner root; the original nine Jetlink socket tests also run in the earlier
native integration stage. Deadlines, packet limits and original tests stay
unchanged. Actual continuous DM comparison is still pending.

### Exit-observation race in retained upload tests (#420)

On `624bfa78`, [run 39461054716](https://github.com/bin9208/openpilot-rust/actions/runs/39461054716)
passed full workspace tests and all four continuous DM IPC/original comparisons.
The subsequent tools gate passed 244 cases but failed the owned EINTR child
cleanup observer with `ProcessLookupError` while reading a disappearing proc
entry. Its fixture now accepts ENOENT/ESRCH or zombie state only; live states
remain false and permission errors propagate. The real strace/owned-child test,
selection and five-second bound remain. Deterministic fast regressions exercise
these outcomes. Runtime upload code and transport deadlines are unchanged.
Acceptance: https://github.com/bin9208/openpilot-rust/issues/420.

### Updated protected base during validation (#418)

While #410 validation was running, PR #418 advanced dev-md to `0682e933`.
Merge that latest base into this feature branch, retaining its LFS endpoint,
loopback checkout regressions, ONNX overrun/camera ownership fixes and tests,
hosted-runner policy, and fail-closed baseline source-policy checks. All
concrete jobs retain the newly established Ubuntu 24.04/ARM hosted placement;
serial execution still comes from needs/max-parallel, not runner counts.
The former one-off ONNX dispatch is covered by the common workspace cadence
suite and the existing Xiaoge native --all-targets group (including camera
pixels); no separate automatic validation entry point is restored.

The baseline source policy retains inherited-document tree identity and
no-renames removal checks, adds exact reviewed CI/test paths and exercises policy boundaries in CI. Actual changed-source admission remains
in delivery preparation, preserving ordinary code PR validation scope. Unknown workflow/test paths,
provider/model sources and prefix lookalikes stay rejected. Runtime dispatch
still requires the completed exact-source marked PR merge, and publication
requires the verified source SHA plus independently successful push CI.
No runtime is requested by this CI change; #418's separate release remains
outside this task. New full PR and post-merge evidence will use the updated base.

The diagnostic actual-source check also passed in the `a0e7ff3c` PR run before
this scope correction. It is not a permanent generic PR gate: baseline reuse
eligibility belongs to delivery, not every ordinary code change. Common CI
runs the same policy's positive/negative regressions; delivery retains the
actual source check before downloads/builds.

Matrix failure admission is explicit: all six architecture matrices combine
max-parallel 1 with fail-fast true. The previous inherited false setting would
have permitted another queued architecture after failure despite serial
execution. Policy regression now requires both settings. GitHub's scheduler
cancels queued/running siblings on matrix failure; final required gates reject
the resulting failed/cancelled group.
Reference: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstrategyfail-fast

The `5a3c548e` [PR run 39476994003](https://github.com/bin9208/openpilot-rust/actions/runs/39476994003)
passed groups 1–10 functional steps, including startup UI and bridge namespace
comparisons, but group 10 cleanup failed on root-owned namespace evidence and
Python bytecode. Cleanup now retries with noninteractive sudo only inside the
already validated exact job-owned temporary root, without crossing filesystems
or following symlinks. A hosted-runner regression creates real root-owned output
and verifies cleanup preserves an external symlink target. The later groups and
ARM build skipped, and both final required summaries failed as intended.
Continue exact-source PR and post-merge validation; acceptance remains in #417.

### GM warmup clock-dependent setup (#444)

On `bc39d162`, [run 39505026003](https://github.com/bin9208/openpilot-rust/actions/runs/39505026003)
passed groups 1–20 and the complete Card/CAN original corpus, then actual Volt
IPC failed at measured CAN row 2: source included steering frame 0x180 while
native did not. Equal topic admission histories had omitted clock-dependent
warmup state: repeated loopback on every fenced setup step made rare host
pauses cross the unchanged 15ms GM transmit guard. Warmup steering indexes
were source `[96]` versus native `[39,89,174,249,317]`, changing last-steer state.

Give only Volt's warmup an explicit pre-first-loopback state, retaining all
other setup frames and all 80 measured corpus packets, including their full
loopback confirmations. In addition to the unchanged measured exact gate,
require the complete warmup CAN bytes/order/validity to match after equal
input histories; mismatches fail immediately without output-driven retries.
Cloud regressions check retained measured inputs and real gate wiring against
tampered warmup output. GM runtime/source code, EPS guard, cadence, readiness,
packet counts, deadlines and input-only three-attempt bound stay unchanged.
Full latest-source PR and post-merge acceptance remains tracked in #444/#409.

### Updated protected base during validation (#446)

While the `d73136a3` run was validating, PR #446 advanced dev-md to `f060fc42`.
Retain all merged ONNX preprocessing/lane changes, frozen source oracles,
whole-process compute benchmark, regression tests and their existing records.
The common workspace and native Xiaoge all-targets checks include the new
crop, lane and native tensor comparisons on both architectures. Move the
exact frozen-oracle/source identity verification from the former manual fast
workflow into the common fast stage; do not restore an independent validation
entry point. The benchmark source remains available, with its recorded ARM
measurement in the original #446 evidence; this task makes no new CPU claim.

The old runtime dispatch exception for `codex/fix-416-onnx-compute` is superseded
by this task's verified exact-source dev-md PR-merge delivery policy. #446's
separate marked runtime delivery stays outside this task. Cancel the outdated
`d73136a3` PR run rather than use its older-base results for merging. It passed
groups 1–33 (including both Xiaoge architecture comparisons) and reached the
native memory group before cancellation. Repeat full PR and exact-merge
post-merge admission on the merged source; do not publish a runtime here.
