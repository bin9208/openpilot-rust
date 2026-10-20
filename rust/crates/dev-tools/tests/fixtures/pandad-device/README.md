# Panda device controller (source-only preparation)

This unit replaces `check_pandad_device.py` and its self-contained
`pandad_device_cases.py` request generator. Both originals remain unchanged.
`check_pandad_ci.py` remains the existing caller; its migration is parent-owned.
The independent C++ source builder and the production Panda policy are untouched.

`contract.json` is explicitly a source transcription, not an executed capture.
Native request generation uses the existing CPython-compatible MT19937 mechanics
exported by `journald_check::Random`, not any journal or Panda runtime policy.
The entire 1,746-case/2,490-operation corpus is generated procedurally on every
run. Seed/rejection order, byte patterns, random CAN bus choices, short/negative
reads, all hardware bytes, signature pairs and repeated serial chunks are kept.

The controller runs the same actual C++ and native probes, with 60-second owned
child bounds and the original complete JSONL/stdout/stderr artifact phases.
It removes only `results[*].health.interrupt_load`, retaining raw transcripts,
interrupt bits, every other field and array order. Python JSON numeric equality,
complete response counts, and zero unconsumed replies are required. Mismatches
retain the first complete source/native row in `mismatch.json`.

Additional evidence protections are exclusive output admission, held regular
file identities opened nonblocking, before/after input seals, fallible stdout,
and atomic no-clobber success publication with inode-owned rollback on cancellation.
Provenance distinguishes direct controller/source pins from the complete input
map already emitted by the original C++ source builder. These are bounded
identity checks, not an atomic snapshot against arbitrary ABA mutation.

Cloud validation is mandatory and cannot silently skip missing prerequisites:
`PANDAD_DEVICE_SOURCE_BINARY`, `PANDAD_DEVICE_NATIVE_BINARY`,
`PANDAD_DEVICE_SOURCE_MANIFEST`, `PANDAD_DEVICE_PYTHON`,
`PANDAD_DEVICE_TEST_OUTPUT` (an existing evidence directory).
The cloud pair runs the unchanged original controller, then the native controller
with disabled PATH against the same probes. It compares the entire independently
generated request roster, whole original reports and every raw child transcript.
It rechecks both probe hashes and the original builder's declared source map.
Native production code never executes Python or loads expected response fixtures.

Seven contract/process-boundary tests and two actual original/native cloud tests
are prepared. No builds, probes, comparisons, tests or Clippy were run locally;
only formatting/syntax/diff checks are allowed under the current user workflow.
No original deletion, shared registration or CI/caller edit is part of this handoff.
