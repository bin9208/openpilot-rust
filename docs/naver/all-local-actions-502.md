# All Actions on local runners (#502)

Issue: https://github.com/bin9208/openpilot-rust/issues/502

The owner explicitly requires every job, including ARM execution, to run on the
registered local runner. This supersedes the earlier GitHub ARM exception.
The existing runner is bin-actions, Linux/X64 with the dev-md-local label.
No hosted x64 or ARM fallback is allowed. Existing unrelated running checks and
successful unchanged checks are preserved; routing applies to the next necessary
execution. Local checkout validation remains syntax/format/diff only.

## Implementation plan

- [x] Inventory current workflows and runner registration; create an isolated
  dev-md issue branch without changing the dirty main checkout or #296 track.
- [x] Add policy regressions before changing routing.
- [x] Run their baseline failure and final success in local Actions, without
  running functional tests locally.
- [x] Change every workflow's runner labels while retaining triggers, disabled
  inherited publishers, required check names and the serial validation chain.
- [ ] Keep original ARM commands and aarch64 binaries, bindings and subprocesses
  in a separate Ubuntu ARM user space through QEMU/PRoot. Preserve exact Python,
  Rust and original dependency pins. Record host and guest architecture and the
  emulated execution mode; do not claim physical ARM CPU validation.
- [ ] Preserve Actions environment-file changes, command exit status, workspace
  paths and owned temporary cleanup through the ARM shell transport. Validate
  actual ARM ELF loading, Python/C bindings, subprocess IPC and signal behavior.
- [x] Require actual self-hosted provenance in packaging without changing source,
  archive, ABI, sealing, installation or publication approval guards. Remove the
  user-deleted fixed disk reserve while keeping estimated-growth rejection.
- [ ] Run focused policy and ARM transport checks on bin-actions, review the
  implementation and validate the exact-source PR through the existing path.
- [ ] Deliver the approved local-only policy to the four ongoing work chats,
  persist it in their continuation records and pass reusable infrastructure to
  #296 without merging dev-md code into dev.

## Evidence and limitations

The preliminary capability request39844532155 was cancelled before execution.
The focused [run39847632228](https://github.com/bin9208/openpilot-rust/actions/runs/39847632228),
source6a8e1c4c8c24d6e98ba404885c789e99476f16a3, executed on bin-actions.
The baseline-red routing regression passed, and all seven new local-runner policy
tests passed. The retained suite ran86 tests with31 errors, all caused by the
focused workflow omitting Node.js from PATH. The regular CI already sets up
Node24; the focused workflow now uses the same setup. This is a validation
configuration error, not runner instability. The failed policy job prevented ARM
execution; no ARM functional success is claimed. No functional checks or builds
have run on the workstation.

On source d27cd41ee8082cd7feb76f8ec59c8d7a9701d37b, the focused
[policy job120074319646](https://github.com/bin9208/openpilot-rust/actions/runs/39850292573/job/120074319646)
passed on bin-actions:86 CI policy tests,9 package-policy tests,13 native disk-guard
tests and4 Rust disk-space unit /5 CLI tests. The baseline-red phase correctly
rejected60 inherited hosted job routes with no harness errors.

The subsequent ARM job120076394086 failed during guest package installation:
Ubuntu's PRoot5.1 let dpkg's database access check fail before unpacking packages.
Upstream added faccessat2 translation in5.4; the helper now pins the official
PRoot5.5.0 host binary by SHA256 and records its version and digest. The original
dpkg operation remains required; the next Actions execution must verify this
compatibility correction before ARM support can be accepted.

The [PR CI39850289766](https://github.com/bin9208/openpilot-rust/actions/runs/39850289766)
failed independently at native dependency setup: the persistent runner retains
the intentionally registered arm64 foreign architecture, while the default
x64 Ubuntu archive does not serve arm64 indexes. Native APT commands now select
the executing environment's dpkg architecture per invocation, including inside
the ARM guest. The explicit dual-source cross-build operation and foreign
architecture registration are preserved. These preparation failures do not
establish runtime defects or justify dropping the original ARM assertions.

The ARM execution transport uses an official Ubuntu Base ARM64 rootfs and the
existing pinned Python 3.12.14 / Rust 1.94.0 tools. Downloads are hash pinned.
PRoot supplies the guest filesystem and QEMU executes ARM user-space code on
the registered X64 host. Physical-device timing or native ARM CPU performance
is outside this CI environment; original functional assertions are retained.

## Implementation snapshot

All workflow runner labels and the saved CarrotMan job template now select
self-hosted/Linux/X64/dev-md-local. The ARM matrix identifies local-x64 and
local-arm64-qemu separately. Its original command bodies remain unchanged except
for the obsolete fixed disk reserve; a custom shell provides the ARM filesystem,
Python/native bindings and subprocess execution. ARM targets and upload paths are
separate from X64. The runner environment/name remain the actual self-hosted
values in all evidence. Policy tests enforce that routing and preserve unrelated
source/ABI/publication guards. These are implemented changes awaiting Actions
execution, not claimed passing results.

Independent GPT-6.1 Sol review identified trust conditions needed on always-run
aggregates, named ARM artifact uploads and a staged radard artifact regression
assertion. Those findings were addressed without dropping ARM commands or tests.
The ARM validation also checks command exit37 propagation and host visibility of
the ARM target before running the retained runtime-core tests. Review is source
evidence only; no functional success is claimed while Actions is queued.

Sources for environment pins: [Ubuntu Base SHA256SUMS](https://cdimage.ubuntu.com/ubuntu-base/releases/24.04/release/SHA256SUMS),
[official Python 3.12.14 release](https://github.com/actions/python-versions/releases/tag/3.12.14-31661455385),
the [PRoot QEMU interface](https://github.com/proot-me/proot/blob/master/doc/proot/manual.rst),
and [PRoot syscall support and releases](https://github.com/proot-me/proot/releases).
