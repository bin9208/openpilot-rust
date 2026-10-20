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
