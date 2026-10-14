# Updater cleanup test observation (#334)

Source `46fb794e6f2b6c3f76c18ac1020ea2c2fe735b04` failed
`duplicate_start_and_close_reap_owned_child_preserve_unrelated` at the
`gone_or_zombie` assertion in [UI CI](https://github.com/bin9208/openpilot-rust/actions/runs/38609201373/job/116009418898).
Twelve other library tests passed. This happened before the separate #329
handshake injection gate. No `/proc` contents or descendant completion receipt
was retained at the failing assertion, so the exact CI interleaving is unknown.

The test used two observations: `!proc.exists() || read_stat.is_ok_and(zombie)`.
If the process disappears between them, exists is true and the failed stat
read yields false, so a stopped/reaped process incorrectly fails the assertion.
An owned sleep process killed/reaped between those two observations reproduced
that false result deterministically. Independently, group SIGKILL delivery does
not require all descendants to finish before waiting for the direct leader
returns. The immediate assertion did not account for that asynchronous exit.
Neither observation proves a runtime process leak in the failing CI run.

## Test-only correction

Capture the owned descendant's pidfd while the fixture is still running and
assert it is not yet readable. After `worker.close`, poll that same identity
for exit, bounded by the original close start plus one second. Retain the
original close-under-one-second assertion and additionally require the whole
close-plus-observation interval below one second. Keep direct leader reaping,
duplicate-start rejection and unrelated-child-alive assertions. A live child
still fails completion; pidfd exit readiness does not depend on disappearance,
zombie reaping, or reuse of the numeric PID.

The unrelated owned sleep now has an RAII guard that kills/waits on normal
return and assertion unwind, with null stdout/stderr. Previously a failing
assertion bypassed its explicit cleanup, leaving its inherited CI output pipes
open for up to60 seconds. The observed CI command ended roughly60 seconds after
the panic, consistent with that possibility but not proof it caused the delay.

Deterministic controls retain a live pidfd negative, then kill/wait the same
child and require completion on that descriptor. An owned temporary directory
models the old exists/read disappearance window without relying on PID reuse.
A caught assertion unwind requires the unrelated guard to have reaped its
child. Production worker process-group signals, waits, updater retry/startup/
reboot logic and all runtime code remain unchanged; helpers live inside
`#[cfg(test)]` only.

Local original focused test passed once; no random repeated run is claimed as
proof of the CI cause. Final source: full UI library15 tests PASS (including
six worker tests and both new controls), strict UI library/tests Clippy PASS,
edition-2021 rustfmt and `git diff --check` PASS. Cached native
process-child was reused; bounded two-job/incremental-disabled builds used the
shared cache with capacity guards. No device access or production deployment.
