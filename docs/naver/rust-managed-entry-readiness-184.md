# Managed-entry collector readiness (#184)

The push validation at commit `6fe58bf9bba938c45e7ebdf36919804c3b748e15`
failed in the source `reset-error` scenario, although the corresponding PR
validation passed. The failure was a missing first `errorLogMessage` in the
comparison fixture.

- Issue: <https://github.com/bin9208/openpilot-rust/issues/184>
- Failed push: <https://github.com/bin9208/openpilot-rust/actions/runs/36895549407>
- Same-commit PR run: <https://github.com/bin9208/openpilot-rust/actions/runs/36895651094>

## Cause and correction

`Peer.start()` waits for the two shared-memory queue files. In original msgq,
file creation precedes `msgq_init_publisher()`. Publisher initialization clears
reader identities and validity. A subscriber created between those operations
reconnects at the current write position on its next receive, losing a message
that was already published. The initial managed-entry debug record exercises
`logMessage` only, so it does not establish readiness of `errorLogMessage`.

The managed-entry comparison now calls the existing `Peer.synchronize()` before
starting a managed child. The debug marker proves that collector initialization
has completed; its error-topic receive reconnects that reader before scenario
records arrive. This changes the QA fixture only. Runtime logging, message
validity, queue sizes, timeout values and scenario assertions are unchanged.

`check_managed_entry_readiness.py` forces this ordering with inherited pipes:
create the actual error queue, construct the parent subscriber, then permit the
original publisher initialization. It runs the unchanged source collector and
the actual source managed-entry scenario. There are no scheduling sleeps in
this injected ordering. The CI startup job runs it before the complete comparison.

## Local evidence (2026-10-02)

Retained local evidence root:
`.analysis/scratch/2026-10-02-port-resume/` in the primary checkout.

- `managed-readiness-red/` and `.log`: the unchanged comparison failed receiving
  `errorLogMessage`. The collector was alive, its error queue contained `crash`,
  and pipe acknowledgements recorded subscriber-before-publisher ordering.
- `managed-readiness-green/`: the same ordering passed after the readiness call.
- `managed-readiness-final/`: final regression helper verification.
- `managed-entry-all/manifest.json`: all 44 original/native comparisons passed
  across both original and Rust collectors, including IPC, Params and SDK checks.

The managed child ELF SHA-256 was
`2fdfb87af449ce4d5a4c81ba7c7f6b951c734138259f4e54c569065e911b03cb`;
collector ELF SHA-256 was
`36492b2c9e704bce30ac950cb9f0518fb1692e2fcf8f8ca3c6021853fa820012`.
The regression helper passes Ruff. The pre-existing managed-entry script has
existing compact-style Ruff findings; the added readiness line introduces none.

Exact-commit cloud validation remains required after publication. Issue #184
must remain open until its CI acceptance criteria are met. These results do not
establish full-runtime startup, device behavior or measured CPU savings.

Docs-Not-Needed: this fixes CI fixture synchronization without changing settings
or user-visible runtime behavior.
