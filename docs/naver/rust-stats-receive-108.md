# Interrupted statistics receive

Issue [#108](https://github.com/bin9208/openpilot-rust/issues/108) was found by
[PR #104 telemetry CI](https://github.com/bin9208/openpilot-rust/actions/runs/36746610648/job/109994281980).
The native runtime exited with `Interrupted system call` while deviceState
messages were being published, before any shutdown signal.

The unchanged Python daemon calls PyZMQ `recv_string`; its `_recv_copy` retries
EINTR. The Rust daemon propagated that error. Controlled `zmq_msg_recv` injection
reproduced the difference in the actual source and native daemon, then passed
after adding only the EINTR retry. Five cases cover one/repeated interruption,
EAGAIN, EINVAL and ENOTSOCK, retaining queued metric bytes and fatal errors.
The production executable also passed real startup, file publication and SIGTERM.
The receive comparison is required in telemetry CI and retains raw traces.

Focused local checks passed. Required PR and separate post-merge CI remain
pending; no device or performance result is implied. Source cadence, publication
and stop-flag checks are unchanged.
