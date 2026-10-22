# Stable-dev local Actions adaptation (#497 / PR #500)

The owner requires every Actions job to use the registered `bin-actions` runner
(`self-hosted`, `Linux`, `X64`, `dev-md-local`). There is no hosted x64 or ARM
fallback. Fork PRs and other repositories cannot schedule these jobs, including
the always-run aggregates and reusable workflow callers/callees. Existing runs
are not cancelled or repeated solely to change routing. This record applies to
the separate stable-dev backlog branch; it does not integrate dev-md runtime
changes or authorize device deployment.

## Exact neutral source

Only these three infrastructure files were selected from
[PR #504](https://github.com/bin9208/openpilot-rust/pull/504), exact head
`6a8e1c4c8c24d6e98ba404885c789e99476f16a3`. No merge, cherry-pick, dev-md workflow,
runtime, packaging code, or commit history was imported.

| File | Original Git blob | Stable-dev delta |
| --- | --- | --- |
| `tools/ci/local-arm` | `c8ae0f44d6ff3430cddf757239e4208835e96be9` | Replace only the two host bootstrap APT calls with `tools/ci/apt-native` |
| `.github/actions/local-arm/action.yml` | `f51440bb48fd6c2d69b503eaf8601a36b103e062` | None; copied verbatim |
| `.github/actions/job-temp/action.yml` | `154566f4016bc4a84a0650aacae93b847f694485` | None; copied verbatim |

The helper's host prerequisite fallback now invokes `apt-native update` and
`apt-native install -y proot qemu-user-static`. Appending `amd64` to an existing
APT architecture list did not remove persistent foreign `arm64`, the actual
pre-test setup failure investigated in #503. The caller-owned `apt-native`
wrapper isolates architecture defaults with a private configuration; it does
not rewrite explicit source `Architectures`, delete shared sources, or remove
dpkg foreign architectures. Such explicit overrides remain caller policy.
All guest bootstrap APT commands remain unchanged inside the ARM rootfs.

## Workflow adaptation

All 14 existing stable workflow files retain their triggers, source gates,
permissions, concurrency settings, command bodies, package pins, matrix logical
values, working directories, fixture deadlines and job timeouts. The five
required names remain `fast checks`, `integration gate`, `check mapped user docs`,
`rust checks`, and `rust aarch64 build`. The inherited publishers remain disabled
in this independent repository by their original source-repository conditions.
The Athena Clippy component fix, hosted-image cleanup guards, native APT calls,
and parent's #505 regression registrations remain present.

The workflow write set is `backlog-fixes-497.yml`,
`carrot-route-vault-publish.yaml`, `fast-checks.yml`, `integration.yml`,
`jetlink.yml`, `naver-apk-tools.yml`, `naver-ci.yml`, `naver-upstream-sync.yml`,
`rust.yml`, `sync.yml`, `tests.yaml`, `user-docs.yaml`,
`wiki-settings-check.yaml`, and `wiki-settings-publish.yaml`, all under
`.github/workflows/`. Policy changes are confined to
`tools/ci/tests/test_rust_isolation.py` and `test_workflow_policy.py`.

The following original ARM execution lanes use the neutral custom shell:

| Stable `rust.yml` job | `LOCAL_CI_ARCH` selection |
| --- | --- |
| `webcam-runtime`, `planner-runtime`, `webrtc-runtime`, `xiaoge-runtime`, `radard-runtime` | Existing logical `matrix.runner` ARM entry selects `aarch64`; x64 entry selects `x86_64` |
| `carrot-man-runtime` | Existing `matrix.arch` |
| `radar-arm`, `carrot-navi-arm` | `aarch64` |

The historical `ubuntu-24.04` / `ubuntu-24.04-arm` matrix values are logical
identifiers only; they no longer choose runner hardware. Existing `matrix.arch`,
`matrix.machine`, host-only sanitizer conditions, `uname -m`, ELF architecture
checks, native Python bindings, source comparisons, IPC and signal lifecycle
commands are retained. Host cross-builds and Miri remain x64 operations with
their original ARM targets; they are not replaced by x64 duplicates.

Each ARM-capable job initializes its owned temp directory, prepares the
transport, and runs its original commands through `openpilot-local-shell {0}`.
`actions/setup-python` remains a host action for the x64 entry only. The guest
uses the foundation's hash-pinned Ubuntu ARM64 rootfs, Python 3.12.14 and Rustup
bootstrap. The original Rust 1.94.0/component installation commands remain.
Two explicit ARM-only setup commands install `file` for webcam and WebRTC,
whose original inspection commands depended on the hosted image providing it.
No original inspection or behavioral command was removed.

The guest preserves workspace and command-file paths while binding a private
ARM target directory over `rust/target`. Artifact actions run on the real X64
host, so ARM binary uploads use `LOCAL_CI_TARGET_DIR`, not the host checkout's
`rust/target`. Radard continues uploading its existing bounded staged binaries.
Each ARM artifact includes `local-arm/execution.json` and `qemu-version.txt`.
The manifest records exact source, runner name/environment, X64 host, AArch64
guest, `qemu-user`, pinned input hashes and `physical_arm_cpu: false`.

## Persistent temp isolation

Every concrete job with a checkout initializes the neutral `job-temp` action
immediately after its first checkout. Its short, fresh `/tmp/openpilot-ci.*`
directory is marked with run ID, attempt and job ownership. `BASH_ENV` directs
Bash-run tools' `RUNNER_TEMP` and `TMPDIR` there, preserving explicit fixture
overrides. Artifact paths use `env.CI_JOB_TEMP`, because `runner.temp` still
identifies Actions' original persistent temp root. This prevents reused names
such as `diagnostic-close-red` from colliding with previous-run evidence.

Evidence upload precedes transport finish and job-temp cleanup. Cleanup rejects
unrelated, nested, symlinked or differently owned roots and removes only the
marked job directory, escalating for root-owned namespace evidence only after
the same checks. It does not reset shared caches or remove another job's output.
Checkout-free aggregates only evaluate prerequisite results and remain free of
temp-producing work; their `always()` conditions explicitly retain the source
trust predicate.

## Verification and remaining acceptance

Policy tests are written for Actions execution, including constant local labels,
fork blocking with `always()` true, reusable callers, preserved ARM execution,
host-visible uploads, neutral blob provenance, repeated job temp roots, stale
evidence preservation and refusal to clean another owner's directory. Existing
source/ABI/fixture assertions remain; obsolete hosted-fallback and physical ARM
runner expectations are replaced by the actual transport contract.

Only YAML/Bash syntax, Python AST and diff review are allowed locally. Functional
policy tests, builds, fixtures and transport execution have not run in this
adaptation session. Parent review and exact-source Actions validation on
`bin-actions` are still required. The common #502 foundation remains owned by
the other chat; this adaptation does not claim its queued probe passed.

Static review parsed 16 YAML files (14 workflows and two neutral actions), 464
Bash command blocks plus the shell helper, and both policy-test Python ASTs.
The workflow diff inventory found 63 concrete jobs using the local labels and
60 checked-out jobs with paired temp initialization/cleanup. All 458 original
workflow command blocks match the pre-adaptation bytes; only the two approved
ARM dependency blocks were added. Original workflow triggers, permissions,
concurrency, job names, dependencies, matrix values, timeouts and existing job
environment values were compared without executing project code.

Actions policy commands:

```bash
python -m unittest discover -s tools/ci/tests -v
python -m pytest -c /dev/null --rootdir=. --confcutdir=tools/ci/tests -q tools/ci/tests/test_native_disk_guard.py
```

The original ARM job timeouts and all runtime/fixture deadlines are unchanged;
emulated execution may reveal an execution-budget limit, which must be reported
without weakening the functional assertions. QEMU/PRoot is ARM user-space
execution on X64, not physical ARM CPU timing, target GPU, AGNOS runtime, vehicle
boot or driving acceptance.

Remaining privileged/global interfaces are unchanged: native package installs;
the generic cross-build's explicit foreign ARM provisioning; root-launched
mount/network namespace comparisons and their evidence ownership; shared tool
installation caches; and inherited `tools/op.sh setup`, which can update system
packages, udev rules and user tools. The temp action bounds output cleanup; it
does not make these operations a host sandbox. The already-guarded image-bundle
deletions never run on the persistent self-hosted runner. Full workflow
execution and any privileged/GPU prerequisites remain parent-reviewed Actions
acceptance, separate from this static adaptation.

The independent #503 host proof passed at `b90ae57c71b4c057bdcf6c8da9359d36d4a020e1` ([run](https://github.com/bin9208/openpilot-rust/actions/runs/39845768239/job/120058499294)): four regression tests plus actual native APT update/install commands on `bin-actions`, with foreign `arm64` still registered. The requested packages were already installed. This does not prove the new stable workflow transport or any runtime gate; those require this adaptation's exact-source execution.

## Required check names for blocked fork events

Independent review found that guarding a job alone leaves a skipped check,
which GitHub accepts for branch protection. All five required jobs now use the
same trust predicate in both their job guard and name expression. Trusted
same-repository events retain each exact legacy required name. Blocked events
use a distinct `Blocked source - ... awaits trusted validation` name; no job
is scheduled, and the exact required name remains unreported/Expected until
trusted validation occurs. Existing `always()`, dependencies and success
assertions are retained. This avoids a skipped-success bypass without a hosted
fallback or any execution of untrusted source on the local runner.

The regression evaluates all five actual workflow name expressions for trusted
push/dispatch/PR and untrusted repository/fork contexts, requiring exact normal
names and no match to any required name for blocked contexts. GitHub's actual
check display still requires execution evidence; no fork PR or external status
was created for this review. See the primary [GitHub required-check guidance](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)
and [conditional job behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions).

### Foundation proof is still incomplete

The owner's later `d27cd41ee8082cd7feb76f8ec59c8d7a9701d37b` [policy job](https://github.com/bin9208/openpilot-rust/actions/runs/39850292573/job/120074319646) passed. Its [ARM transport job](https://github.com/bin9208/openpilot-rust/actions/runs/39850292573/job/120076394086) stopped in guest package preparation with `dpkg: error: required read/write access to the dpkg database directory /var/lib/dpkg`. No ARM lifecycle proof passed. The selected neutral helper requires the owning #502 track's repair and a successful execution proof before this full candidate is published. This is an execution-foundation failure, not an application runtime result. The host-only #503 proof above remains valid for its own exact source and scope.
