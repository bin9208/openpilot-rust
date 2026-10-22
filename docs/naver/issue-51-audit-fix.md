# Issue #51: recover from skipped-file xattr marking errors

Scope: [#51](https://github.com/bin9208/openpilot-rust/issues/51), assigned under
[#497](https://github.com/bin9208/openpilot-rust/issues/497). Same isolated worktree
and branch as [#46 record](issue-46-audit-fix.md); baseline
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

## Mechanism and desired outcomes

Empty files and exact-name oversized `qlog`/`qcam` files intentionally skip HTTP.
Their original xattr error handler accesses uninitialized `last_exc`, raising
`UnboundLocalError`; Rust explicitly returned `UninitializedLastException` and
terminated the daemon. Transferred files instead returned success on marking
failure, but logged `exc=None` rather than the actual xattr error.

The Rust handler now reports the actual marking error for either branch. Failed
marking of a skipped file returns `Outcome::Failure`: its attribute remains absent,
its filename is not recorded as a successful transfer, and ordinary retry/backoff
and signal handling continue. Successful marking still completes the intentionally
skipped file without sending data or emitting `upload_success`. A transferred
file retains the existing successful return and last filename on marking error;
its missing attribute still permits later selection. Failed transfers never
attempt marking. No HTTP status, deadline, size cap, file priority or stop/backoff
algorithm changes.

The removed fatal error variant and its trace adapter arm have no remaining Rust
references. The transfer-error diagnostic variable is scoped to its existing
failure branch; its contents and logging behavior are preserved.

## Regression and compatibility exceptions

Tests were written before implementation. `tests/filesystem.rs` exercises the real
uploader/selection/backoff code with empty qlog, oversized qlog and oversized qcam,
two permission-failed attempts, unchanged last successful filename, recovery,
actual xattr completion and then idle selection. The transfer dependency panics
if a skipped file attempts network I/O. A transferred-file test retains successful
return while requiring actual marking-error diagnostics.

The existing policy/HTTP differential checker still compares all unaffected
results and xattrs. Its only policy exception requires the exact original
`UnboundLocalError` outcome, one unuploaded skipped file, configured marking
failure, no last filename, and exact pre-error events; it then requires one false
result and `uploader_setxattr_failed`. The logging checker changes only the
transferred mark-error record's `exc` expectation from None to real permission
error details; all other fields, levels, ordering, callsites and collector checks
remain intact. Original Python source is unchanged by this Rust-only assignment.

The continuous daemon checker additionally runs real read-only empty/oversized
qlog files as the unprivileged hosted user, requires repeated marking errors,
no HTTP requests or upload-success events, absent xattrs, normal backoff and clean
SIGTERM/SIGINT shutdown. Files exist before startup to avoid the unrelated long
idle backoff.

## Hosted commands and RED preservation

Run only in GitHub-hosted Actions with existing support-runtime dependencies,
from the repository root:

```sh
export PYTHONPATH="$RUNNER_TEMP/support-msgq-python:$PWD:$PWD/rust/tools"
cargo test --manifest-path rust/Cargo.toml -p openpilot-uploader --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-uploader -p openpilot-logmessaged --bins --examples --locked
python rust/tools/build_msgq_python.py --output "$RUNNER_TEMP/support-msgq-python"
python rust/tools/check_uploader.py --binary rust/target/debug/examples/uploader_trace --output "$RUNNER_TEMP/issue-51-policy.json"
python rust/tools/check_uploader_daemon.py --binary rust/target/debug/openpilot-uploader --output "$RUNNER_TEMP/issue-51-daemon"
python rust/tools/check_uploader_logging.py --binary rust/target/debug/openpilot-uploader --trace rust/target/debug/examples/uploader_trace --collector rust/target/debug/openpilot-logmessaged --output "$RUNNER_TEMP/issue-51-logging"
python rust/tools/check_uploader_timeouts.py --binary rust/target/debug/examples/uploader_trace --output "$RUNNER_TEMP/issue-51-timeouts"
python rust/tools/check_uploader_http_interrupted.py --binary rust/target/debug/openpilot-uploader --output "$RUNNER_TEMP/issue-51-http-interrupted"
```

For actual RED, retain all candidate tests/checkers in a disposable hosted checkout
but restore only original production files before building:

```sh
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/uploader/src/lib.rs > rust/crates/uploader/src/lib.rs
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/uploader/examples/uploader_trace.rs > rust/crates/uploader/examples/uploader_trace.rs
cargo test --manifest-path rust/Cargo.toml -p openpilot-uploader --test filesystem --locked
```

Expected RED: skipped marking returns the fatal error at the test's `expect`,
and transferred marking lacks real exception details. Build the old daemon and
run the candidate continuous checker for independent real-syscall RED as well;
it must detect the daemon's nonzero exit. Rebuild the unmodified candidate in a
separate GREEN job. Parent's aggregate validation should also retain existing
uploader log-socket fault checks and unchanged broader CI.

## Evidence status

Local checks: syntax, formatting and diff review only. No local tests/builds or
fixtures were run. No commits, pushes or GitHub writes. Exact-SHA hosted RED/GREEN,
Actions URLs, aggregate verification and issue acceptance remain parent-owned.
Device execution was not attempted and cannot be inferred from host checks.

Docs-Not-Needed: recoverable internal marking failures; no settings changes or
paired user-guide edits were assigned.
