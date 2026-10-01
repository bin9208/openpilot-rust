# Original journal comparison timeout (#127)

The unchanged source `journald.py` occasionally exceeded the existing five-second
EOF shutdown check. The failure was reproduced with the synthetic child alone;
no production daemon change is made.

The initial occurrence was at `1fa52dcc` in
[job110045330712](https://github.com/bin9208/openpilot-rust/actions/runs/36761615792/job/110045330712).
At `47dea85b`, the original `empty-eof` lifecycle case failed in
[job110245905007](https://github.com/bin9208/openpilot-rust/actions/runs/36824101141/job/110245905007).
Artifact11144248300 retains a `stdout-closed` child record, followed about five
seconds later by SIGTERM records whose parent PID is already1. Cleanup killed
the timed-out parent before those records.

Locally, original daemon trial140 reproduced the timeout. Before cleanup,
`/proc` showed the parent in `wait4(child_pid)` and the child blocked in stdin
`read(0,...)`, with SIGTERM unblocked and no pending kernel signal. A separate
child-only EOF/terminate loop failed at trial299. Adding syscall tracing changed
the outcome:200 full-daemon trials and1,000 child-only trials passed. These
observations isolate the intermittent failure to the synthetic child's signal
and blocking-input handling; they do not establish a CPython implementation bug.

The fixture now uses `signal.set_wakeup_fd` with an owned nonblocking pipe and
`select` over stdin and that pipe. The Python signal callback performs no I/O.
The ordinary loop records termination after finishing the current trace append,
preserving the ordering required by #74. A signal arriving before `select`
leaves the pipe readable; one arriving during the wait wakes it. The old
signal-mask and buffered stdin iterator are no longer needed.

The comparator now captures both processes' `/proc` status, wait channel,
current syscall and open descriptor targets **before** cleanup on a timeout.
Unavailable or exited-process reads are recorded as errors. The original
timeout still raises, and all existing source/native exit and child ownership
assertions remain required. No retry, larger deadline, or production change is
introduced. A future failure's diagnostic files travel with the existing CI
artifact so its blocked resource can be identified. An intentionally stopped
owned source process verified all eight diagnostic files are captured while
the timeout still raises.

After the final fixture change,1,000 direct child EOF/termination trials passed.
The complete unchanged source/native journal comparison passed:155 messages,
13 malformed-JSON errors,19 fatal-input cases and4 lifecycle cases per side.
Three focused tests cover signal-during-buffered-append ordering, EOF followed
by SIGTERM, and termination while a1MiB output write is blocked. The tests use
real pipes and retain the five-second timeout. Ruff and whitespace checks pass.
The first local full run supplied the wrong binary filename; the next used an
old binary whose embedded source commit failed the expected provenance check.
Both failed attempts are preserved. `full-green3` uses a freshly built native
binary and is the complete passing comparison.

Exact-SHA CI and post-merge checks remain parent gates; artifacts live in the private
`.analysis/scratch/2026-10-01-rust-journal-fixture/` evidence directory.

Track resolution in [#127](https://github.com/bin9208/openpilot-rust/issues/127).
The inherited direct-SIGTERM orphan behavior remains separate in #66.

Docs-Not-Needed: host comparison diagnostics only.
