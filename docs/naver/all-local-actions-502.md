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
- [ ] Run their baseline failure and final success in local Actions, without
  running functional tests locally.
- [ ] Change every workflow's runner labels while retaining triggers, disabled
  inherited publishers, required check names and the serial validation chain.
- [ ] Keep original ARM commands and aarch64 binaries, bindings and subprocesses
  in a separate Ubuntu ARM user space through QEMU/PRoot. Preserve exact Python,
  Rust and original dependency pins. Record host and guest architecture and the
  emulated execution mode; do not claim physical ARM CPU validation.
- [ ] Preserve Actions environment-file changes, command exit status, workspace
  paths and owned temporary cleanup through the ARM shell transport. Validate
  actual ARM ELF loading, Python/C bindings, subprocess IPC and signal behavior.
- [ ] Require actual self-hosted provenance in packaging without changing source,
  archive, ABI, sealing, installation or publication approval guards. Remove the
  user-deleted fixed disk reserve while keeping estimated-growth rejection.
- [ ] Run focused policy and ARM transport checks on bin-actions, review the
  implementation and validate the exact-source PR through the existing path.
- [ ] Deliver the approved local-only policy to the four ongoing work chats,
  persist it in their continuation records and pass reusable infrastructure to
  #296 without merging dev-md code into dev.

## Evidence and limitations

The preliminary local capability probe is run39844532155, source
bba2408fb30c840906cc18de74273d1866b783f8. It is queued behind other authorized
local work at the time of writing. No host dependency install, build, functional
check or ARM execution has run locally on the workstation.

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
and [PRoot QEMU interface](https://github.com/proot-me/proot/blob/master/doc/proot/manual.rst).
