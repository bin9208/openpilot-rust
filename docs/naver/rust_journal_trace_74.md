# Synthetic journal trace signal ordering

Issue [#74](https://github.com/bin9208/openpilot-rust/issues/74) was found during
the final [support integration #71](https://github.com/bin9208/openpilot-rust/issues/71)
comparison at `3bb36bd88af30c3f80361a3ceb9305ac631e366a`.

The original journald completed with exit0 and reaped its child. All155 normal
androidLog packets and13 error records matched. The synthetic journalctl trace
ended `started, signal-15, stdout-closed`, although the recorded close timestamp
preceded the signal timestamp. This failed the existing last-event assertion.

The fixture's buffered append was interrupted by SIGTERM. Its handler opened
another append stream, wrote and flushed the signal event, then raised SystemExit.
Unwinding the interrupted context flushed its older event after the signal event.
This was a test-fixture reentrancy bug, not a demonstrated journald shutdown bug.

The deterministic regression runs the real fixture in a child process. A narrow
file-write wrapper sends a real SIGTERM after the close event is buffered and
before its context flushes. Before the correction it reproduces the same final
`stdout-closed` event. The fixture now blocks SIGTERM only while appending and
flushing a trace record, then restores the previous signal mask. The handler's
event therefore follows the completed interrupted append. Production code,
fixture payloads and all journal comparison assertions remain unchanged.

Validation in the isolated issue worktree:

- Focused regression: fails before the correction, passes afterward.
- Full configured Rust Python-tool test suite:19 passed, none skipped.
- Original/native journal comparison:155 packets and13 errors each, all19 fatal
  input cases and the existing lifecycle scenarios pass.
- Focused Ruff and diff checks pass.

The initial local suite attempt lacked MODEL_RUN_BINARY and the CPU/LLVM settings
already configured in CI; its15 setup-dependent failures remain recorded.
The configured run passes all19 tests. An earlier invocation used a Python
environment without pytest; that setup error is retained separately.

Evidence is retained under `.analysis/scratch/2026-09-30-rust-journal-trace/`:
`red-test.log`, `green-final-test.log`, `tools-tests-configured.log`, and
`native-green/report.json`. The original failed captures remain in the support
integration's `final-support/support-runtime/journald/original/normal/` directory.
No real OS journal, vehicle or production process selection was changed.

Docs-Not-Needed: internal deterministic test fixture and engineering evidence only.
