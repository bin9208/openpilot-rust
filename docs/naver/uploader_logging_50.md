# Legacy uploader diagnostic parity follow-up

Issue [#50](https://github.com/bin9208/openpilot-rust/issues/50), under the full
runtime port, retains the original uploader's diagnostic schema and levels as
well as its file/HTTP decisions. The review base is
`0992326494ce93d6b1ad1911919119c75390ff29`.

The actual original `SwagLogger`/`SwagFormatter` and local HTTP server reproduced
three missing contracts: a DEBUG upload URL/header message, failed HTTP response
representation as a string such as `<Response [500]>`, and the two-string `exc`
list containing error representation and trace. The native process also printed
an extra `uploader: ready` line and attributed records to its forwarding helper.
The old decision oracle retained event names only and could not establish these
logging contracts.

The transfer now emits the DEBUG record through the uploader's existing event
sink before FAKEUPLOAD or PUT, preserving header insertion order and Python-style
representation. Structured upload failures retain INFO severity, response strings
and a null or two-string exception field. Rust reports its typed error and an
actual Rust backtrace captured at the reporting site; it does not fabricate the
Python exception text or imply that the backtrace is the unwound failure stack.
Filesystem exception text includes the affected path and native error information.

Every record carries the actual Rust producer's file, line and function into the
logging producer. ERROR exception records retain `exc_info`; ordinary upload
failures do not become ERROR records. The source logger's caller-attribution quirk
for direct INFO calls is recorded separately from Rust's explicit producer site.
The extra readiness text is removed. Continuous tests synchronize through original
msgq reader registration, which also occurs after startup lock cleanup.

## Verification

`rust/tools/check_uploader_logging.py` runs actual source uploader/main functions
and the original formatter against the same synthetic HTTP/filesystem fixtures.
It exercises the continuous native executable directly into both original and
Rust collectors, plus the existing trace executable for direct getsize/listdir
failure boundaries. Eighteen cases per collector cover missing identity,
unsuccessful status, malformed JSON, accepted response statuses, fake upload,
zero/oversized files, xattr/metadata/lock errors, and ordered/quoted/Unicode/nested
header representations.

Comparison checks all message fields, scalar/list types, levels and record order.
Temporary root paths and measured transfer speed are normalized. Error text is
checked for its real language/error category and affected path; language-specific
traces are not compared as equal strings. Native callsites resolve to the actual
`log_site!()` lines and producer functions. Actual cereal `logMessage` and
`errorLogMessage` packets and typed disk records are retained. Both collectors'
disk transformations are compared with the unchanged `SwagLogFileFormatter`:
DEBUG reaches `logMessage` but stays out of disk, and INFO transfer failures stay
out of `errorLogMessage`. Default-console silence is checked for nonfatal records
below WARNING.

```sh
cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-uploader -p openpilot-logmessaged --bins --examples --locked
PYTHONPATH=/path/to/original-msgq-binding:.:rust/tools python rust/tools/check_uploader_logging.py \
  --binary rust/target/debug/openpilot-uploader \
  --trace rust/target/debug/examples/uploader_trace \
  --collector rust/target/debug/openpilot-logmessaged \
  --output /path/to/uploader-logging-evidence
```

The original/current binaries and records are indexed separately under
`.omo/evidence/uploader-logging/evidence.json` in the issue-50 worktree. Existing
source policy, signed HTTP, real ten-second timeout, continuous IPC and generic
ARM checks remain relevant. Parent integration owns the workflow update and
exact-SHA cloud results. No active Carrot upload transport, production selection,
vehicle connection, real credential or external upload was changed or used.

Docs-Not-Needed: correction of internal optional-uploader diagnostics; no settings
or user workflow changes.
