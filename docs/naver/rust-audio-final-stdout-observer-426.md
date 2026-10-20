# Audio QA stdout boundary observer (#426)

The exact 2d8 process test timed out five seconds after a source mutation.
It did not publish a root success receipt. A second cloud run of source
`0d1700e5a3ad0181ecfdd7f7f25e4fc9d088e1d2`,
[job118894126192](https://github.com/bin9208/openpilot-rust/actions/runs/39499806776/job/118894126192),
retained detailed phase observations in artifact12302035000. The parent
independently downloaded/read the archive (SHA-256
`07b4dc3866b10ed862129c3ba76e11d219df7d82f98162ac24d01f845fb3d080`).

At the predicate, mutation and five-second timeout, `/proc` reported the
checker running, with syscall text `running`, `wchan=0`, no stderr and no root
receipt. Only the65,536 prefilled `x` bytes were drained; the final controller
line had not appeared. The bounded daemon receipt and provenance already
existed. Thus the old500ms-after-provenance predicate did not establish a
blocked stdout write. It could inject the mutation during the preceding
verification. This is not evidence of an accepted stale result or cleanup
failure. A separate cancellation case also missed its10-second fixture-open
deadline before durable worker evidence existed.

The captured daemon ELF was93,730,640bytes, whereas21 pinned sources totaled
184,044bytes. Repeated debug-build artifact hashing is a supported diagnostic
target, but these observations do not identify the exact instruction or
measure its CPU cost. The next source uses the existing workspace's
`profile.{dev,test}.package.sha2.opt-level=3` settings for checker/test/Clippy
builds, preserving the hash implementation, pins and all deadlines. The source-matched f60e4ac0 cloud execution below establishes the observed
process-test result; it does not measure a general device CPU benefit.

The mutation observer now requires the live checker, both prerequisite
receipts and no root receipt, then proves a sleeping Linux `write` or `writev`
on fd1 through `/proc/PID/syscall`. The pipe remains full and unread until the
private mutation, so that write cannot complete before the mutation. x86_64
and aarch64 syscall numbers are explicit; unreadable or unsupported observation
fails instead of falling back to a sleep or skipping the test. The original
30-second observation and five-second rejection deadlines, source and daemon
mutations, owned cleanup, and saved stdout/proc snapshots remain.

The separate retained-binding finalization repair is tracked in
[#433](rust-audio-qa-finalization-433.md). No local functional test or device
audio test ran. Both repairs now have the exact-source cloud acceptance recorded below;
the latest integration commit must still pass its required checks.


Source`f60e4ac0a655592aaef659f35cc354b86ffa09a0`
[passed the full audio job](https://github.com/bin9208/openpilot-rust/actions/runs/39505980261/job/118914882870).
The8process checks include actual blocked-fd1 source and daemon mutations,
controller cancellation, broken stdout,15CLI statuses and6failure boundaries.
The full process suite completed in56.86s with no failure. Parent reviewed the
complete log and artifact12304571676 (ZIP SHA256
`23e074eef387fbff6bf1262993e0854536833183f5c4a593611bb6f78f8a8652`).
The existing30s observation/five-second rejection limits remain unchanged.
The original/native seven modes, strictClippy and PortAudioMiri passed in the
same job. This resolves the observed hosted-test timeout for that exact source;
latest integration acceptance and physical-device audio remain separate.
