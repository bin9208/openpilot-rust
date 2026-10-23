# Panda safety comparison I/O budget (#518)

## Reproduced boundary

PR #500 source `10919538d31b04d28d9a23b383d925441434d579` passed native
Panda builds/tests/lint and original protocol comparisons before the original
safety oracle exceeded the checker's 90-second whole-batch allowance in
[job 120220822773](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220822773).
Native safety comparison had not begun. The original timeout path raised
before saving partial stdout/stderr.

The independent registered diagnostic source
`587320a45b3133bbd8710887cd594fa28e2b7f46` collected evidence in
[run 39924193562/job 120316453762](https://github.com/bin9208/openpilot-rust/actions/runs/39924193562/job/120316453762).
Its successful job means evidence collection, not source/native acceptance.
Artifact `12490410914` is a 123,341-byte ZIP authenticated against the Actions
API SHA-256 `4a861bed5de347af135100641cd30f4a66f2481521ed383169aaf5d31625a8b5`.

It reused the exact original ASAN/UBSAN executable SHA-256
`8f019be68c4d19311344d9db016b1ed36846905145345b33eda865f3a8a9e7f0`
and all 897 inputs / 4,499 operations. Only the fresh owned Params root changed;
all other input fields remained identical, on the same physical ext4
filesystem. Original input SHA-256 was
`20af4658f3f42d016780e7f2ba4bc7ea8afc1ac1802cddbb464c203eee75c11b`;
root-independent input SHA-256 was
`30530de82eb620081b63f24a323b78c3d44351587bd61a4dc8594244fb8e4258`.

The untraced run timed out after 90.0166 seconds with only 0.734654 seconds of
source user/system CPU. It completed 862 rows / 3,624 operations, next
`seeded-15`, retaining 1,151,041 stdout bytes without truncation. Of 163 source
process samples, 97 reported `jbd2_log_wait_commit` and 102 reported syscall
74 (`fsync`). The owned process group was gone and its wrapper reaped.

A separate bounded `strace -f -c -w` observation also timed out and produced
byte-identical partial stdout. Its 14,197 `fsync` calls used 88.350460 seconds,
99.81% of the 88.518291 seconds spent in the selected traced syscalls;
10,088 `flock` calls used 0.043338 seconds. Tracing overhead is distinct from
untraced timing. Together these observations identify aggregate durable-I/O
waits exceeding the old batch allowance, without establishing a physical disk
fault, software deadlock, full completion time, or a product safety defect.

## Focused correction

Keep `invoke`'s default 90-second allowance for CarParams and the separate
imported padding probe. Give only the two full source/native safety batches
180 seconds each. Native timing is not yet measured; the matching budget
allows the same full workload and durable Params operations. This changes a
finite CI comparison allowance, independently of product runtime deadlines.

Save partial stdout/stderr byte-for-byte and an invocation receipt before
re-raising `TimeoutExpired`. Save the same receipt on process completion,
including requested allowance, monotonic elapsed time, return code and capture
sizes. A completed process is not a successful comparison: the existing full
output-length, JSON and source/native assertions still follow it.

All 897 scenarios, 4,499 operations, strict equality checks, eight explicit
truncation/recovery controls, padding checks, original binary/sanitizer
configuration, physical Params writes and synchronization remain intact.
No tmpfs, write suppression or product safety-policy changes are introduced.

Local checks are AST syntax and diff review only. Actual source completion,
native timing/equality and complete Panda transport/supervisor acceptance must
run in registered Actions on the final candidate. Required checks, normal
integration and separate post-merge validation remain pending.
