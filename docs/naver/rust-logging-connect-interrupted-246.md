# Interrupted logger connection during Athena upload (#246)

Xiaoge PR #245 at `526f62a5b6be6a80f821500cc24ce3e61262eda2`
[failed its Athena transfer gate](https://github.com/bin9208/openpilot-rust/actions/runs/37805691466/job/113409244086).
Only three of four owned 18,000-byte upload requests reached the receiver.
The missing `/hold/1` followed `upload_start` and an `Interrupted system call`
exception. This differs from the older redirect-timeout observation in #214.

The retained syscall stack identifies the interrupted operation as a new
logger's `zmq_connect` mailbox wait. It occurs before the missing upload opens
its HTTP connection. The test's SIGUSR2 interrupts that wait; a different upload
thread's interrupted HTTP read is retried correctly. A focused syscall-injection
control reproduces the native logger failure and missing collector packet.
The unchanged Python handler succeeds because pyzmq retries the same connect
operation on InterruptedSystemCall.

The correction retries only `zmq::Error::EINTR` from `socket.connect`, using
the same context and socket. Other connection errors still propagate. No Athena
HTTP behavior, upload retry policy, timeout or comparison assertion changes.

Six owned source/native runs pass: ordinary connection, the actual interrupted
connect poll, and an invalid-protocol control for each implementation. Successful
emits deliver exactly one record and preserve context/socketpair counts. The
existing Athena transfers scenario passes in 16.69 seconds with all four held
uploads on their first attempt, metered deferral/recovery, disconnect retry,
truncation behavior and socket options retained. Fifteen logging tests, two doc
tests, strict Clippy, formatting, Ruff and 21 CI-policy checks pass.

The new regression runs inside the existing Athena job, which already supplies
strace; it adds the selected logging probe and the existing pyzmq version pin.
Its tracer uses `--kill-on-exit` and always closes stdin. A forced early failure
preserves the exception, reaps the tracer and confirms the exact owned tracee's
exit through a pidfd. No whole Athena corpus was repeated locally.

Evidence and source/binary identities are retained in this issue checkout's
`.omo/evidence/246-checkpoint/receipt.json`; the primary checkout also retains
the executed final checks under `.omo/evidence/246-executor-verification/`.
Exact-head integration and post-merge CI remain required. This host correction
does not establish device acceptance or full-runtime startup/upload completion.
