# Uploader logging error boundaries (#68)

The native uploader's `RuntimeEvents` previously converted every record/transport
error to stderr and continued. This differed from the original uploader's ordinary
`cloudlog` calls. A closed logger at `upload_start` let Rust issue GET and PUT and
mark the file uploaded, while Python stopped before either request. A closed logger
at the URL DEBUG message let Rust PUT and mark the file; Python caught the exception
inside `do_upload`, skipped PUT, emitted `upload_failed`, and applied backoff.

The reproduction executes original `SwagLogger`, `SwagFormatter` and
`UnixDomainSocketHandler` against a genuinely closed ZMQ socket. The native fixture
uses the public Logger close contract and the actual `RuntimeEvents` implementation.
A one-call fault recovers to a healthy sink for later diagnostics; a persistent
fault also tests failure while reporting `upload_failed`. Neither fixture replaces
logging operations with a fake returned error. Before the fix, 22 of 48 source/native
collector comparisons differed, and two native regression tests failed.

## Correction

`EventSink::emit` now returns `Result<(), openpilot_logging::Error>`.
`RuntimeEvents` returns both record-conversion and logger errors. Ordinary uploader,
scan, lock-cleanup and runtime INFO/error calls propagate them at the same boundary
as Python. Scanning and next-file selection return a `Result` so a diagnostic
failure cannot silently turn into an empty scan. `HttpTransfer` converts its DEBUG
logging error into `TransferError::Logging`; the existing upload catch path handles
it. EAGAIN remains the shared logging producer's nonfatal dropped-message result.
HTTP behavior, xattr ordering, backoff, timeouts and the original uninitialized
`last_exc` behavior are unchanged.

## Direct verification

`rust/tools/check_uploader_log_fault.py` compares 15 scenarios against both original
and native log collectors, running the source and native paths: 60 observations per
architecture. Cases include normal success, real queue saturation, first and
persistent DEBUG failure, backoff-log failure, upload start/success/ignored/failed,
oversize, metadata/listing/getsize, lock cleanup and marking-error diagnostics.
The source step cases run the actual original `main` with disposable Params and
substituted network-state/sleep boundaries. The native fixture invokes the actual
Uploader, HttpTransfer, RuntimeEvents and Backoff components. The failed operation
returns a nonzero process exit. The fixture reports its real source-language
traceback/callsite rather than borrowing Python provenance for Rust.

Checks compare operation result/exit, last filename, attempted log calls, exact
record sequence/levels, HTTP GET/PUT occurrence and uploaded xattr. Collector
checks compare actual cereal publications with persisted disk records. DEBUG-only
failure produces GET without PUT, `upload_failed` with the real socket error, then
backoff 0.2 s from the initial 0.1 s. Persistent failure stops at `upload_failed`.
EAGAIN is triggered by filling the actual disconnected ZMQ queue until its native
send returns Again/Dropped; both implementations still complete the upload.

Host and aarch64/QEMU comparisons pass. Additional unchanged regressions pass:
212 filesystem cases, 10,000 backoff decisions, 32 loopback HTTP cases (RS256/ES256,
byte-exact compression), and 36 normal native-daemon/collector logging scenarios.
Eleven native tests and targeted Clippy/format/Ruff checks pass. The full generic
aarch64 uploader and its examples build successfully.

Evidence is recorded in `.omo/evidence/uploader-log-errors/evidence.json` in the
issue worktree, including the preserved pre-fix executable, failed native tests,
source/native comparison JSON, raw cereal packets, disk files, commands, source
hashes and executable hashes. The Cython binding and collector from frozen #64 are
read-only dependencies, identified by hashes in the ledger.

This is a local runtime parity fix. No production selection, workflow, shared
Params or #61 implementation changes are included. There was no device access,
real credential use, external upload or push. Parent integration owns cloud checks
and the full-runtime first-device-test gate.
