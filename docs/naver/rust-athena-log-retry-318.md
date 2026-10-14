# Athena log scan error retry pacing (#318)

Tracking: [#318](https://github.com/bin9208/openpilot-rust/issues/318).
The module audit identified an inherited error-loop hazard in Athena's non-PC
log worker. A missing or persistently unreadable Swaglog directory makes `logs`
fail before the scan timestamp is advanced or the normal response-queue wait is
reached. The outer loop logged the error and immediately scanned again. The
original `openpilot/system/athena/athenad.py::log_handler` has the same branch;
this is not described as a newly introduced Rust regression or an observed
vehicle outage.

The native worker now waits one second through the existing interruptible
`Stop::wait` after a failed cycle. Successful cycles receive no additional delay.
The existing ten-second successful-scan interval, log order, active-file
exclusion, file contents, submission/response queues and xattr acknowledgement
rules retain their behavior. A failed cycle does not advance scan state or
discard pending log state merely to suppress retries. Existing per-file
rotation failures remain handled within the normal cycle.

## Reproduction and validation

An isolated original-source reproduction extracted only the two log forwarding
function bodies using AST, supplied an owned missing directory and stopped after
three errors. It executed three failures with zero waits. It imported no
original runtime modules and contacted no server. Its concise result is in
ignored `.analysis/scratch/2026-10-14-athena-retry-318/original-busy-retry.json`.

The native RED fixture uses actual `logs` filesystem reads against an owned
missing directory, with deterministic cycle/wait observation. Before the fix it
failed with `persistent IO failure retried without waiting`: three attempted
cycles and zero waits. After the change it observes one one-second retry wait
per failed cycle without creating the missing directory. The injected wait makes
the failure/cadence proof independent of host scheduling or a CPU benchmark.

All four Athena tests pass:

- Persistent missing-directory failures yield between attempts.
- Successful cycles receive no retry wait.
- A real healthy forwarding fixture preserves newest-active-file exclusion,
  newest-first send order, CR/LF normalization, JSON fields, pending-file order
  and the exact successful acknowledgement xattr bytes.
- Requested stop interrupts the real failure wait, returns within the
  test's 500 ms bound and performs no subsequent attempt. `Stop::wait` uses its
  existing at-most-100 ms sleep slices; no new shutdown mechanism is introduced.

`cargo test --locked -p openpilot-athena` and strict all-targets package Clippy
with `-D warnings` pass; Rustfmt and the scoped diff check pass. Host builds used
the coordinated target cache, two jobs and disabled incremental compilation,
with 375–376 GiB free before each build. The parent also rebuilt the forwarding example and passed the unchanged
original/native forwarding and startup-clock-boundary comparisons: actual
WebSocket log/stat forwarding, order, newline conversion, ACK timestamps and
cleanup matched. The isolated checker dependency `websockets==15.0.1` was
installed under ignored scratch after a disk guard. Other Athena session
integration suites were not rerun locally; the full source-matched candidate
build and required CI still apply.

All filesystem tests used owned host fixtures; no device, production log
directory or real network delivery was exercised. Privacy/retention policy,
control/numerical validity, scheduling and settings are unchanged. This
fix bounds a demonstrated retry path; it does not establish measured CPU savings,
complete Athena/module clearance or startup/upload/drive acceptance.

Docs-Not-Needed: internal failure retry pacing; no setting or user-guide contract
changes.
