# Stable-dev local Actions adaptation (#497 / PR #500)

The owner requires every Actions job to use the registered `bin-actions` runner
(`self-hosted`, `Linux`, `X64`, `dev-md-local`). There is no hosted x64 or ARM
fallback. Fork PRs and other repositories cannot schedule these jobs, including
the always-run aggregates and reusable workflow callers/callees. Existing runs
are not cancelled or repeated solely to change routing. This record applies to
the separate stable-dev backlog branch; it does not integrate dev-md runtime
changes or authorize device deployment.

## Exact neutral source (initial version 1)

Only these three infrastructure files were selected from
[PR #504](https://github.com/bin9208/openpilot-rust/pull/504), exact head
`6a8e1c4c8c24d6e98ba404885c789e99476f16a3`. No merge, cherry-pick, dev-md workflow,
runtime, packaging code, or commit history was imported.

| File | Original Git blob | Stable-dev delta |
| --- | --- | --- |
| `tools/ci/local-arm` | `c8ae0f44d6ff3430cddf757239e4208835e96be9` | Replace only the two host bootstrap APT calls with `tools/ci/apt-native` |
| `.github/actions/local-arm/action.yml` | `f51440bb48fd6c2d69b503eaf8601a36b103e062` | None; copied verbatim |
| `.github/actions/job-temp/action.yml` | `154566f4016bc4a84a0650aacae93b847f694485` | None; copied verbatim |

The initial helper's host prerequisite fallback invokes `apt-native update` and
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
Each ARM artifact includes `local-arm/execution.json`, `qemu-version.txt`, and
the version 2 helper's `proot-version.txt`.
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

### Version 1 failure before the version 2 proof

The owner's later `d27cd41ee8082cd7feb76f8ec59c8d7a9701d37b` [policy job](https://github.com/bin9208/openpilot-rust/actions/runs/39850292573/job/120074319646) passed. Its [ARM transport job](https://github.com/bin9208/openpilot-rust/actions/runs/39850292573/job/120076394086) stopped in guest package preparation with `dpkg: error: required read/write access to the dpkg database directory /var/lib/dpkg`. No ARM lifecycle proof passed. The selected neutral helper requires the owning #502 track's repair and a successful execution proof before this full candidate is published. This is an execution-foundation failure, not an application runtime result. The host-only #503 proof above remains valid for its own exact source and scope.

### Neutral helper update, version 2: prepared, ARM proof pending

From clean local stable-dev head `b263f5e82a297bf28df8497ea22c14a1e48973a8`,
the selected helper is updated to the exact #504 source
`77663836a95a9e2aff9aa66592d8b06ed9c87cd9`, Git blob
`d93c939703117c27f7320cc2182214dded09bcae`. Only `tools/ci/local-arm` is copied
from that source; no branch history, dev-md runtime or other workflow is
imported. Both neutral action files remain unchanged, with the same blobs
`f51440bb48fd6c2d69b503eaf8601a36b103e062` (local-arm) and
`154566f4016bc4a84a0650aacae93b847f694485` (job-temp).

The owning foundation replaces Noble's PRoot 5.1 with a private downloaded
PRoot 5.5.0, SHA256
`db185300a9daabbce86aca7632d8c7fdc4249f6f153171954013b70c5f491ec0`.
This addresses modern glibc `faccessat2` translation implicated in the guest
dpkg database access failure; the new source also explicitly checks guest
read/write access to `/var/lib/dpkg`. This is the foundation's proposed repair,
not a claim that its ARM proof passed. The private binary is used directly,
its hash is included in `execution.json`, and all eight ARM artifact uploads
now include `local-arm/proot-version.txt`.

The stable adapter differs from this new neutral helper by exactly two host
APT substitutions:

```text
sudo -n apt-get -o APT::Architectures=amd64 update
  -> "$source_dir/tools/ci/apt-native" update
sudo -n apt-get -o APT::Architectures=amd64 install -y qemu-user-static
  -> "$source_dir/tools/ci/apt-native" install -y qemu-user-static
```

The host no longer installs a distribution PRoot package. Guest APT commands
remain byte-identical to the new neutral source. The provenance regression
normalizes only these two host calls back to the new neutral blob and retains
the unchanged action-blob checks. ARM upload assertions require PRoot version
evidence. No other workflow command, check name, trust condition, architecture
gate, fixture or deadline changes in this follow-up.

At preparation, the owner's [probe 39853582741](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741)
is **in progress / PENDING**, not passed. Actual ARM transport/lifecycle proof
remains a prerequisite for publishing the new PR #500 head. This preparation
does not push the local candidate or change the remote `afed` source. Local
review remains syntax/AST/YAML/Bash and diff only; no functional tests, builds,
fixtures, transport execution or Actions dispatch run in this session.

### Version 2 foundation proof succeeded; candidate gates remain pending

The exact `77663836a95a9e2aff9aa66592d8b06ed9c87cd9`
[probe](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741)
completed successfully, including [policy job 120085864083](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741/job/120085864083)
and [ARM job 120086462570](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741/job/120086462570).
The completed log and [artifact 12459003068](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741/artifacts/12459003068)
were read back. The manifest identifies `bin-actions`, X64 host, AArch64 guest,
`qemu-user`, `physical_arm_cpu: false` and the exact PRoot/rootfs/Python hashes.
The ELF header is AArch64; ARM Python 3.12.14 loaded its real native binding.
Environment propagation, host-visible isolated target, exit code 37, subprocess
socketpair IPC and SIGTERM exit 23 all passed. Evidence upload and owned cleanup
completed successfully.

The ARM Rust disk-space CLI ran, and the runtime-core harness compiled and ran;
its only lib test is an ignored syscall benchmark, so that harness reported
**zero active unit tests**, not runtime-core behavioral coverage. The existing
full original/native and ARM workflow gates are retained and still required.
This proof admits the neutral execution interface; it does not validate this
backlog candidate's runtime, physical ARM hardware, AGNOS or vehicle behavior.

The stable adapter keeps only its two host APT substitutions, whose helper has
independent actual host proof at `b90ae57c7` above. With the common transport
proof now available, publishing the prepared #500 source can proceed to its own
exact-head focused regressions and all five required pre-merge checks. Normal
merge and post-merge evidence remain required before closing its code issues.
