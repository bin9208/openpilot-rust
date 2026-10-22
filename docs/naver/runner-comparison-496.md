# GitHub Actions runner comparison — issue #496

Completed 2026-10-23 KST. The owner requested a direct speed and reliability
comparison, followed by removal of the obsolete local disk-capacity rule.

- Issue: https://github.com/bin9208/openpilot-rust/issues/496
- Fixed tested source: `dc95138076836ea3a5c1b6c27262be3f3b912f71` (`dev-md`).
- Hosted job: https://github.com/bin9208/openpilot-rust/actions/runs/39821849474/job/119974886265
- Local runner job: https://github.com/bin9208/openpilot-rust/actions/runs/39822667561/job/119977729615
- Hosted workflow SHA: `718b41fcba6e31fee8e5ad204cd0bf4bf0b366a4`.
- Local workflow SHA: `0f963807b41c1d6cbffdc994ec747c75188bf7ad`.
- Policy removal commit: `30c7c34c01693eb0b2fb23684306bfff01504672`.

## Conditions and scope

Both jobs checked out the fixed source above and the identical msgq submodule.
Both used the identical Python package versions, Rust1.94.0, two Cargo build
jobs, no incremental compilation and disabled debug information. Each job used
fresh isolated Cargo/Python/Gradle dependency caches, then three rounds with a
fresh Rust target each round. Android used `--rerun-tasks --no-daemon`; all nine
tasks executed in every round. Existing global tool installations and source
checkout caches differ, so overall job duration is not a strict cold-host ratio.
The recovery run disables checkout cleaning to preserve pre-existing untracked
files; the tested source and workload commands are otherwise identical.

| Runner | CPU available to job | RAM | Job outcome |
| --- | --- | --- | --- |
| GitHub `ubuntu-24.04` | 4 logical CPUs, AMD EPYC7763 | about16GiB | success |
| `bin-actions` | 4 online logical CPUs, i9-13900 | 4GiB | success |

The existing CI/wiki/Jetlink Python checks, Android core/runtime tests, and
runtime-core/msgq tests with native C++ IPC peers passed in all three rounds
on both runners. The additional warm Rust test invocation passed every round.
The exact FFmpeg6.1.1 APT simulation resolved on both runners; it does not prove
FFmpeg installation or full native video build compatibility.

## Timings

Seconds; three-round medians except explicitly separated Android cache states.
The speed ratio is GitHub time divided by local runner time.

| Workload | GitHub | Local | Local speed ratio |
| --- | ---: | ---: | ---: |
| Existing fast Python checks | 37.559 | 16.592 | 2.26x |
| Rust/C++ clean target compilation | 13.512 | 6.655 | 2.03x |
| Native Rust/IPC test execution | 3.094 | 2.896 | 1.07x |
| Warm native test rerun | 3.084 | 2.898 | 1.06x |
| Android first round, fresh Gradle dependency cache | 89.416 | 72.424 | 1.23x |
| Android rounds2–3, mean, tasks forced to rerun | 25.978 | 10.282 | 2.53x |

The three fast-check samples were37.516–38.702s on GitHub and16.532–16.768s
locally. Clean Rust/C++ compilation was12.848–14.806s and6.362–6.820s,
respectively. No functional test, compile, package-resolution or upload failure
occurred in these completed workload rounds. This small sample does not establish
long-term runner availability or full-workspace memory adequacy.

Overall job durations were6m25s and3m49s. They include different initial tool and
source-cache states and are supplementary observations, not the controlled ratio.
Artifacts contain all21 stage timings per runner, exit codes, package versions,
source/submodule bindings, host specifications and complete stage logs:

- https://github.com/bin9208/openpilot-rust/actions/runs/39821849474/artifacts/12445182367
- https://github.com/bin9208/openpilot-rust/actions/runs/39822667561/artifacts/12444479431

## Initial policy block and owner correction

The first local job stopped before checkout/build because an obsolete repository
rule required35GiB free and the runner had30GiB. This was a policy rejection,
not a runner crash, disk-full error or functional failure. Its parent run is
therefore marked failure even though the paired hosted job passed.

Read-only diagnosis found an11GiB ignored `rust/target` cache. No compiler was
active, no tracked files were inside it, and only this inactive build cache was
removed under the owner's existing cleanup authorization. Free space reached
41GiB; the existing untracked `.omo/` directory was preserved. Recovery evidence:
https://github.com/bin9208/openpilot-rust/actions/runs/39822375121 .

The owner then explicitly removed the policy. `AGENTS.md` no longer requires
25/35GiB fixed free-space thresholds, mandatory pre-local-build capacity checks,
or their associated bounded-build instruction. Cache cleanup authorization and
source preservation remain. The future comparison workflow also has no fixed
free-space gate. A queued recovery dispatch was cancelled only to avoid an
experiment concurrency lock; the completed local run above is the actual retry.

## Decision and limits

For this fast-check and small native-build scope, both runners worked and the
local runner was roughly twice as fast for compilation and Python checks.
FourGiB RAM did not prevent these workloads; full ARM/AGNOS/provider builds and
long-running heavy integration were not tested. Keep the recommendation within
that scope. No production CI routing, release, device access or dev/main
integration was performed. Only syntax and diff review ran on the workstation.

The experiment remains on `codex/feat-496-runner-comparison`; no runtime promotion
or workflow migration is implied by this result.
