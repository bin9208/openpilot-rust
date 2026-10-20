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
build. Every architecture matrix has max-parallel 1. Existing runners, native
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
