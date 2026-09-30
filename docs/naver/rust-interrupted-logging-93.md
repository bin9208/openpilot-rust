# Interrupted native producer sends (#93)

Issue [#93](https://github.com/bin9208/openpilot-rust/issues/93) tracks the
support-runtime failure in [PR #89](https://github.com/bin9208/openpilot-rust/pull/89),
base `4c78b015b110db9d9fbec52550df25609b105595`.
The failing [Actions run](https://github.com/bin9208/openpilot-rust/actions/runs/36715443317)
reported `Interrupted system call` during the uploader's `force-none-metered`
scenario, after HTTP GET/PUT and before the qlog completion marker.
That cloud stderr alone does not identify the interrupted API or signal source.

## Reproduction and scope

The isolated reproduction instruments `zmq_msg_send` at its real C ABI and injects
EINTR before it accepts the selected `upload_success` packet. The pre-change
native uploader exits with `Logging(Transport(Interrupted system call))` after
GET/PUT and before marking the qlog. The packet trace retains errno, flags,
process/thread IDs and exact bytes. The typed stderr line used to localize this boundary is retained only in the
preserved pre-change binary; it was removed from production source.

The unchanged Python `UnixDomainSocketHandler.emit` and `StatLog._send` catch
only EAGAIN themselves. Their pinned PyZMQ 27.2.0 `_send_copy` implementation
retries `InterruptedSystemCall` internally. Rust zmq 0.10.0 returns that error to
its caller. Native Python-style logging and statistics producers now perform
the same EINTR retry, while retaining EAGAIN drop and other error propagation.
Retries must operate on the already formatted packet; they must not print the
console record or apply logging context again.

`rust/tools/zmq_send_boundary.c` is a test-only send interceptor. Python loads its
shared object against the exact PyZMQ libzmq library. Native test binaries link
its wrapper with `--wrap=zmq_msg_send`. Normal production builds do not link it.
`check_interrupted_send.py` and `interrupted_send_reference.py` exercise real
PUSH/PULL sockets and unchanged original producer definitions. The cases inject
one/repeated EINTR followed by success, EAGAIN, EINVAL or ENOTSOCK, checking
actual call sequence, packet identity, delivery count and console count.

## Evidence status

The preserved pre-change reproduction is under the isolated worktree's
`.omo/evidence/interrupted-logging-93/boundary/`. The initial cloud log and
published artifact are under `.omo/evidence/interrupted-logging-93/cloud/`.
The original/native 20-case producer matrix first failed on the native
single-EINTR case, then passed after the two producer retry loops were added.
The pre-change matrix separately records all ten native first-attempt EINTR
failures. The actual uploader now completes force-none-metered with three
interrupted success-record sends: two GET/PUT pairs, two accepted success
records, both upload markers and exit zero.

`check_uploader_interrupted.py` repeats that full uploader scenario and compares
accepted C-ABI packets with the actual collector packets. Existing uploader
HTTP-deadline, collector/log-fault and producer regression suites remain required.
The final exact-commit evidence ledger is
`.omo/evidence/interrupted-logging-93/final/evidence.json`; remote CI is owned by
PR #89 integration and is not replaced by local evidence. This document does not
claim that a particular signal caused the original cloud failure.

This fix does not change the separate C++-style native logger policy, production
daemon selection, upload eligibility, HTTP retry rules or fatal transport errors.
No device access or device comparison is part of this work. The complete runtime
conversion and first user device comparison remain separate delivery gates.
