# Athena traced startup identity (#170)

[PR #167's Athena job](https://github.com/bin9208/openpilot-rust/actions/runs/36857217941/job/110352492079)
at `999334fdd4c543ba5f401d1bb40897e2b938a865` selected transient tracer child
8891, while the native trace started at 8892. The fixture failed resolving the
first PID, then cleanup raised ProcessLookupError. Teardown removed the owned
build metadata while the real daemon was starting, causing its subsequent
invalid-metadata message. This does not establish a production metadata defect.

An owned tracer fixture reproduced the failure by creating a short-lived helper
before the actual native executable. The readiness check now examines current
owned children and selects the expected executable. Startup exit is reported
explicitly. The fixture creates an owned process group, preserves the real
daemon's graceful SIGTERM path and cleans up remaining owned children without
masking the original error when a PID has already exited.

The new regression verifies the selected PID differs from the transient helper,
matches the native executable, and is reaped. A second case proves tracer exit
7 remains the reported startup error. The actual Athena executable also passes
authenticated WebSocket/RPC/fragments/ping, HTTP retry/upload, statistics,
Params, reconnect and SIGTERM against private peers. Python Ruff passes on
the changed files; no production runtime code or timeout was changed.

Evidence is retained under the ignored Bluetooth workspace:
`athena-identity-red-second`, `athena-identity-final`, `athena-startup-green`;
the downloaded original CI archive is in `2026-10-01-bluetooth-ci/athena-pr`.
The regression is part of `check_athena_runtime.py`, before other scenarios.
Fresh exact-head and post-merge CI remain required. No device was accessed.

Docs-Not-Needed: owned validation lifecycle repair only.
## Evidence upload permissions

Run 36861309481 passed every Athena source/native scenario and codec sanitizer,
then failed artifact upload because the root-run identity fixture created its
owned `strace` script with mode 0700. The unprivileged Actions uploader could
not read that script. Mode 0755 preserves execution and permits evidence
collection. The script contains no credentials; daemon and test file ownership
remain unchanged. Local identity checks and archive creation pass, including
the script's other-user read bit. Local sudo requires a password, so the
distinct-root/uploader identity check remains assigned to the exact-SHA CI run.

## Tracer metadata publication

[Run 36863031862](https://github.com/bin9208/openpilot-rust/actions/runs/36863031862/job/110372448620)
selected the real daemon successfully, then read the tracer's earlier metadata
before it contained the native PID. The result was `KeyError: native` in the
assertion fixture. A controlled 200 ms delay between child launch and atomic
metadata replacement reproduced the failure. The assertion now waits for the
native key using the existing five-second fixture timeout, while the startup
check continues to identify the actual executable. The controlled delay stays
in the regression; production startup and deadlines are unchanged.

Local delayed checks and Ruff pass. Evidence is in the root ignored
`2026-10-01-bluetooth-ci/identity-publish-{red,green,final}` directories.
The earlier evidence-permission repair also passed the complete Athena job
and artifact upload in run 36862959318 at
`736d392c93418d4fe37edc6613b79f76b63b7246`.
