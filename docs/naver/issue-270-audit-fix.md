# #270: main Carrot Web terminal session cleanup

Tracking: [#270](https://github.com/bin9208/openpilot-rust/issues/270), under
[#497](https://github.com/bin9208/openpilot-rust/issues/497).
Base production source: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

## Failure and ownership

`Generation::drop` only sends HUP/KILL to the leader's process group.
Interactive Bash job control creates other process groups in the same session.
HUP-resistant jobs in those groups can survive reset/final shutdown; retained
slave descriptors can also prevent the owner loop from seeing EOF after Bash
exits. WNOWAIT leader reservation (#265) fixes identity reuse, not this coverage.
The original Python `_terminate_proc_locked` likewise signals the leader group;
its behavior is not a correctness oracle for this deliberately repaired case.

The recovery-only subreaper cannot be enabled directly in the multithreaded
main server: that changes orphan adoption for unrelated server services.
Instead each generation re-executes its installed `openpilot-carrot-server`
sibling with `--terminal-supervisor`. Both `src/main.rs` and
`src/bin/carrot_command.rs` route this private mode before managed-entry or
Tokio initialization. It does not start another application, logging/crash
reporting owner, or managed-entry lifecycle. The executable stays an existing
installed artifact; no additional binary, runtime package item, or receipt
format is introduced. The PTY metadata PID remains the actual shell session
leader, not the supervisor PID.

Only this dedicated process enables PR_SET_CHILD_SUBREAPER. It launches the
unchanged shell/session/PTY policy through the existing process-child launcher,
then releases its own slave descriptors. On reset, final control EOF, or CLOSE:

1. HUP/KILL the retained leader group without adding a graceful window.
2. Observe leader exit with WNOWAIT before the first session census, preventing
   an empty census racing a final leader fork.
3. Scan the retained session, signal only direct adopted children validated by
   waitid(WNOWAIT), and reap them. Repeat as nested parents die and their
   children become adopted. No wait-any or unrelated-child reaping is used.
4. Reap the leader only after cleanup. The server waits for the supervisor.

Natural leader exit is checked within a 50 ms control-read interval. The helper
then performs the same descendant cleanup, allowing the actual owner read loop
to receive PTY EOF. It keeps the zombie leader until generation closure so
status/signals never use an identity released by an earlier poll/reap.
The descendant cleanup deadline remains two seconds as in recovery; errors
are reported rather than treated as success. Cleanup is scoped to the owned
session, not deliberately detached sessions. The implementation uses waitid,
/proc and ordinary signals, without requiring pidfd or a newer AGNOS kernel.

## Regression cases and hosted execution

`pty_cleanup_tests.rs` was introduced before the cleanup implementation and
uses the production Generation/State/read/write paths. It starts an interactive
Bash, enables job control, installs HUP resistance, and verifies three live
job identities across at least two nonleader groups (including a nested child).
Cases cover reset via State::finish, owner state destruction, retained exited
leader status, and EOF after shell exit. Each checks complete child removal,
leader reaping, and an unrelated direct child's survival/reapability. The
server process's subreaper setting is checked unchanged.

`supervisor_entry_tests.rs` additionally runs both actual installed executable
entry points under unusable inherited managed-entry configuration, checks
/proc executable and parent/session identities, then closes and reaps them.
The unchanged existing exit-status tests exercise the same mapping function
used by the supervisor.

Run **only on GitHub-hosted Actions**, from repository root with existing
Carrot server native build dependencies installed:

```sh
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-process-supervision -p openpilot-carrot-server --bins
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib terminal:: -- --nocapture --test-threads=1
cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --all-targets -- -D warnings
```

For an actual baseline RED, in a disposable hosted checkout retain the new
`pty_cleanup_tests.rs` but restore only production `pty.rs` from the base, then
append its test-module declaration. No new production helper is used by that
original Generation:

```sh
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/carrot-server/src/terminal/pty.rs > rust/crates/carrot-server/src/terminal/pty.rs
cat >> rust/crates/carrot-server/src/terminal/pty.rs <<'TEST_MODULE'

#[cfg(test)]
#[path = "pty_cleanup_tests.rs"]
mod cleanup_tests;
TEST_MODULE
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --lib terminal::pty::cleanup_tests::issue270_ -- --nocapture --test-threads=1
```

Expected RED mechanisms are surviving owned job identities and missing PTY EOF,
not a compiler failure. Restore candidate source for GREEN. The fixture records
PID/start-time identities and performs emergency owned-job cleanup on failure.
Retain the unchanged transport/source comparison controls, including:

```sh
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-carrot-server --example carrot_terminal
python -P rust/tools/carrot_server_terminal_edges.py --native "$PWD/rust/target/debug/examples/carrot_terminal" --launcher "$PWD/rust/target/debug/openpilot-process-child" --output "$RUNNER_TEMP/issue270-reset" --mode reset
python -P rust/tools/carrot_server_terminal_edges.py --native "$PWD/rust/target/debug/examples/carrot_terminal" --launcher "$PWD/rust/target/debug/openpilot-process-child" --output "$RUNNER_TEMP/issue270-eof" --mode eof
```

No existing source comparator, output equality, threshold or fixture expectation
was waived. The intentional differences are complete same-session cleanup on
reset/final shutdown, and cleanup/EOF after leader exit with surviving jobs.
Shell argv/environment, absence of controlling-TTY acquisition, geometry,
WINCH targeting, metadata leader PID and negative-signal exit codes stay intact.

## Evidence and remaining acceptance

Local checks are limited to Rust parsing/formatting, manifest syntax and diff
review. No local compilation, tests, PTYs, process fixtures, performance work,
GitHub writes, commit, push, or device access occurred in this worker.

Parent must obtain hosted baseline RED and candidate GREEN, existing actual
HTTP/WebSocket/PTY controls and exact-source checks before asserting #270 fixed.
No hosted run or device acceptance is claimed by this source record.

## Worker patch paths

All implementation paths are under `rust/crates/carrot-server/`:

- `Cargo.toml` (existing rustix dependency's stdio feature only)
- `src/main.rs` (one early routing call)
- `src/bin/carrot_command.rs` (route before its existing Tokio wrapper)
- `src/terminal/mod.rs`
- `src/terminal/pty.rs`
- `src/terminal/pty_cleanup_tests.rs`
- `src/terminal/supervisor.rs`
- `src/terminal/supervisor_client.rs`
- `src/terminal/supervisor_entry_tests.rs`
- `src/terminal/supervisor_session.rs`
- `src/terminal/supervisor_session_tests.rs`
- `src/terminal/supervisor_status.rs`

The only documentation path is this `docs/naver/issue-270-audit-fix.md` record.
Shared CI, root manifests/lockfile, recovery and other workers' files were not
edited by this worker. No separate supervisor binary remains in the manifest.

## Independent-review follow-up: bounded close and census identity

Bernoulli identified three P2 failures in the initial source patch. The follow-up
changes only terminal implementation/tests and this record:

- The client no longer reads a CLOSE acknowledgement and then calls an
  unconditional `Child::wait` on the Tokio owner. It queues CLOSE plus write-EOF,
  then retains the helper Child under a guard. The guard uses `try_wait`, sends
  SIGCONT every 20 ms while the child is still owned, and permits the helper to
  finish its own descendant cleanup. A single five-second close deadline
  includes escalation: SIGKILL at 4.9 seconds, followed by nonblocking reap
  observation until five seconds. If kernel exit is still pending, the Child
  moves to a dedicated reaper thread; the async owner does not wait indefinitely.
  Escalation/nonzero exit is reported as failure, never successful cleanup.
  Startup uses the same guard on every error/timeout, closes its listener first,
  and resumes an owned stopped helper during the existing ten-second admission
  deadline. This preserves descendant cleanup for the actual SIGSTOP failure
  rather than immediately destroying the subreaper with SIGKILL.
- The session census now retains `(PID, starttime)`. After waitid(WNOWAIT)
  establishes direct-child ownership, it re-reads starttime and SID before either
  signalling or reaping. A child that called setsid since census, or a stale
  starttime selection, is skipped; original ownership remains intact.
- `/proc/PID/stat` is read as bytes. Only numeric structural fields after the
  final right parenthesis are decoded; arbitrary comm bytes cannot abort the
  census. ENOENT/ESRCH are treated as transient disappearance. Other unexpected
  errors and malformed structural fields remain errors.

The helper's two-second session-cleanup deadline now also covers leader-exit
observation, which uses WNOWAIT|WNOHANG instead of a blocking waitid. A retry
shares the original deadline rather than granting a fresh two seconds. Existing
census completion, ownership restrictions, and cleanup limits were not waived.
A kernel task that cannot yet complete SIGKILL cannot be synchronously guaranteed
to exit by a userspace deadline; the delayed reaper keeps its Child ownership
without blocking the terminal owner. Actual stopped-helper cleanup is a required
hosted test assertion, not an inference from that fallback.

Additional hosted runtime cases (included by the existing `terminal::` filter):

- Stop the actual helper with SIGSTOP while its shell and nested job-control
  groups are alive; drop the generation, assert bounded return, all descendants
  removed, helper reaped, and unrelated child preserved. A six-second emergency
  SIGCONT rescue prevents the faulty pre-review wait from hanging CI forever.
- Stop a pre-handshake wrapper, resume through the production startup path, and
  verify cleanup. Separately run an actual helper executable that never connects,
  use a short injected admission deadline, and assert escalation/reaping within
  the unchanged five-second close budget.
- Keep an unrelated real shell alive with a non-UTF-8 comm containing a right
  parenthesis, then close a real PTY generation and verify cleanup/survival.
- Census a real direct child, let it exec setsid into another session, then call
  the production per-member cleanup path and verify the original owner can still
  kill/reap it. A companion live-child case rejects a stale starttime snapshot.

Use the same hosted build/test/clippy commands above; no new workflow is needed.
The two per-member cases exercise the cleanup function directly to order the
census/setsid race deterministically. The original-production RED procedure
remains applicable to the original lifecycle module; the stopped-helper case
first rejects the original no-helper topology before it can send SIGSTOP to the
runner. These additions are source-complete but unexecuted locally. Parent still
owns hosted RED/GREEN, actual PTY transport checks and acceptance.
