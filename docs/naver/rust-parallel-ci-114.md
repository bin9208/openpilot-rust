# Parallel workspace validation

[Issue #114](https://github.com/bin9208/openpilot-rust/issues/114) follows the
user's 2026-10-01 request for faster Actions-first execution.

[Dev run 36726718097](https://github.com/bin9208/openpilot-rust/actions/runs/36726718097)
spent about 13 minutes on prerequisite runtime jobs, then about 27 minutes on
workspace checks. The same workspace commands now start independently, alongside
the runtime jobs. The required `rust checks` name remains an always-running
aggregate gate; it requires every runtime job and the workspace job to succeed.
The independently required ARM job and all other jobs are unchanged.

Eight CI policy tests pass, including missing/failed/cancelled/skipped result
rejection. A structural comparison confirms unchanged workspace commands,
environment, timeout and artifacts, plus unchanged remaining jobs and workflow
triggers. No validation was removed. Actual time savings depend on runner
availability and are not yet measured for the new job graph.

Docs-Not-Needed: CI scheduling only, with unchanged runtime and user settings.
