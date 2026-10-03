# Athena forwarding fixture clock boundary (#165)

PR #164 at `7a555bb5dbe87a372be7058b73dcbb5765296ef5` failed in
[the Athena CI job](https://github.com/bin9208/openpilot-rust/actions/runs/36853287232/job/110339721232).
The source comparison fixes `now` and gives file `c` an upload timestamp exactly
3600 seconds earlier. The subsequent live daemon uses real time. Crossing a
second makes `c` eligible, so the first two `forwardLogs` requests become `c`
and `b`, before `a` has an upload attribute. The fixture read of `a` then raises
`ENODATA`. This is independent of controlsd and does not establish a runtime defect.

An unchanged test with a 1.1-second delay before starting the native daemon
reproduced the same `ENODATA` locally. The source comparison and its exact
one-hour boundary remain intact. Before the live phase, mark `c` acknowledged;
that phase now contains exactly the intended `a` and `b` pending files.
The suite also runs the live comparison with a 1.1-second startup delay.
Normal and delayed runs passed with actual WebSocket requests, log contents,
ACK attributes, statistics removal, temporary-file preservation and shutdown.

Local evidence is retained in the ignored analysis workspace under
`2026-10-01-controls-athena-forwarding-ci` (CI archive and reproduction) and
`2026-10-01-rust-controls-integration/forwarding-fixed-{normal,delay}`.
`git diff --check` passed. A full Ruff invocation reported five pre-existing
findings in the original forwarding fixture (wall clock required for upload
timestamps, three long lines and the synchronous loop predicate binding).
No production behavior, thresholds or user settings changed.
