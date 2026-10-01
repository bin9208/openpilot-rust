# Interrupted Athena TCP connection (#163)

Tracks [#163](https://github.com/bin9208/openpilot-rust/issues/163) and the
Athena conversion in [#146](https://github.com/bin9208/openpilot-rust/issues/146).

At `f996f37e9f9a448bd97ae225e8b3268f76afa74d`, the
[push Athena job](https://github.com/bin9208/openpilot-rust/actions/runs/36841136429/job/110300690814)
started four uploads, received three PUT requests and logged an interrupted
system call for the other upload. Its socket-option trace does not identify
the failing syscall. A sibling PR run, ordinary local transfers and six local
signal-stress trials passed. The exact source of the original CI interruption
remains unproved.

The pending TCP connection path independently reproduced a source/native
defect. A port-scoped host fixture starts a real loopback connection and then
returns EINTR once. Original Python `_do_upload` completes the full 32,768-byte
payload and returns HTTP 200. Before the repair, the native upload returns
`Http(Io(Os { code: 4, kind: Interrupted, ... }))` without sending a PUT.
Rust's standard connection helper returns an initial connect EINTR directly;
its retry of interrupted readiness waits does not cover that initial call.

The shared native connector now owns the pending socket through interrupted
connect and readiness waits, checks the socket's connection error, and restores
blocking operation on success. Elapsed time reduces the original deadline.
Both upload and WebSocket/proxy callers use this connector. Upload retry,
queue, cancellation and timeout policies remain source-compatible.

## Verification

- The previously failing fixture now produces one source and one native PUT,
  each with SHA-256
  `e11360251d1173650cdcd20f111d8f1ca2e412f572e8b36a4dc067121c1799b8`.
- A 150 ms direct connection deadline expires after 159 ms while every native
  readiness wait is forced to return EINTR. Refused connections retain their
  `ConnectionRefused` result. These are host timing observations.
- The narrow C fixture is built with UndefinedBehaviorSanitizer. No new unsafe
  Rust or C production boundary was added.
- All 17 Athena runtime scenarios pass with the newly built native binaries,
  including uploads, source RPC, cereal IPC, reconnect, proxy backpressure,
  camera lifecycle and supervisor behavior. Strict package Clippy passes.
- `check_athena_runtime.py` requires the interrupted-connect comparison in its
  own network namespace. Transfer traces now retain network/read/write/poll
  syscalls so a subsequent CI failure can identify its actual boundary.

Private evidence is retained under `2026-10-01-rust-athena-integration`:
`ci-failure-36841136429`, `connect-red`, `connect-boundaries-green`,
`connect-full-suite` and their command logs. The original host fixture failure
and the CI incident are recorded separately; the fixture is not proof that
the CI interruption occurred in connect. Exact-head CI and post-merge receipts
are still required before closing this issue.

Docs-Not-Needed: implementation-language compatibility repair; no setting or
intended user-visible behavior change. Full-runtime and vehicle acceptance
remain open under #1.
