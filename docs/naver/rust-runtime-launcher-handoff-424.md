# Runtime launcher failed handoff cleanup (#424)

The launcher owns a spawned manager while it waits for the repository-lock
handoff. `send_boot_lock` has a two-second acknowledgement timeout and the
initial connection has a 60-second deadline. Before this change, an error in
either phase dropped `OwnedChild`, which sent TERM and then called unbounded
`wait()`. A manager ignoring TERM could keep that failure path open forever.
Printing from `Drop` could also wait indefinitely on a full stderr pipe.

Failure and unwind cleanup now allows 500 ms after TERM, sends KILL while the
unreaped `Child` still owns the PID identity, and polls for reap for at most one
more second. If the kernel cannot reap the child within that bound, the owned
`Child` handle moves to a background reaper; cleanup does not print. The
normal successful handoff still enters `finish`, waits for the manager's real
exit, and forwards INT/TERM as before. The repository lock remains governed by
its original file descriptor and any transferred descriptor; this change does
not release it early.

The focused regression in `rust/crates/runtime-launcher/tests/handoff.rs`
launches the test executable as an immutable owned child. The child registers
its TERM handler, connects to the actual lock socket, writes a private token/PID
acknowledgement, and withholds the lock acknowledgement. The parent opens a
pidfd and verifies the child's environment token before observing the real
`run_locked` failure. The test requires a bounded return, child exit and reap,
and continued ownership of the launcher's repository lock. Existing tests
cover successful lock transfer and manager cleanup after forwarded shutdown.

Validation remains cloud-owned. The copied regression and explicit rustix event
feature are first compiled against the old launcher source at
`edb42bf095d6a3e3b38cc0a4546976dfb710363e`. Its token-verified pidfd watcher
kills only its own resistant helper after six seconds, allowing the old drop
path to return and fail the actual `error cleanup exceeded bound` assertion.
The focused fixed test must return in under five seconds without that rescue.
A 15-second external timeout with a two-second KILL fallback is secondary
containment; timeout or compilation failure does not count as the required
baseline failure. The entire fixed handoff target and strict Clippy follow.
The child also watches its parent's pidfd, avoiding a surviving fixture after
an externally interrupted test process. Exit and reap are checked using the
held pidfd, not an unreserved `/proc/PID` pathname.

The workflow reserves 1,024 MiB for baseline checkout, 1,024 MiB for compilation,
and 512 MiB each for focused execution, fixed tests and Clippy, on top of the
existing free-space floor. Record exact source and Actions results in #424.
No local build or functional test was run. If the kernel cannot reap after KILL
and the background reaper thread itself cannot be spawned, the bounded error
path still returns; later reap is not guaranteed in that resource-exhaustion
case. Successful-manager lifetime and user-owned device verification are unchanged.
