# msgq sanitizer checker oracle

`original.json` freezes 18 independent cases from the unchanged
`rust/tools/check_msgq_sanitizers.py`, SHA-256
`4533611f5fdeca25d7f0bd740cf7d0e35894005eee95ccfce3ca43fd809b9d7d`.
The capture uses controlled subprocess responses, retaining original command
order, environment overlays, captured/inherited stream choices, and the first
failure boundary. Paths are normalized to `$RUST` and `/controlled`.
The generator and actual instrumented-run evidence are private under
`.analysis/scratch/2026-10-15-msgq-sanitizers/`.

All four emitted artifacts run in emission order; duplicate artifact paths are
accepted when the count is four. Non-string truthy executables fail only when
the corresponding process is attempted, after libasan lookup and any preceding
tests. Strict UTF-8 and universal newline conversion apply only to captured
stdout. Inherited output remains raw bytes. Python `splitlines` and `strip`
include their Unicode and U+001C..U+001F cases.

The native checker adds the repository disk admission policy, two Cargo jobs,
and disabled incremental compilation, retaining the sanitizer flags and all
other inherited environment variables. Build and compiler lookup have bounded
1,200-second and 30-second guards; original test deadlines remain 60 seconds.
Owned process-group cleanup also kills descendants on failure or timeout.
These bounded host-validation guards replace formerly unbounded preparation;
they do not change completed normal test behavior.

`streams.json` is a separate real-shell capture of the unchanged original CLI,
using controlled Cargo/g++/test programs (no compiler invocation). It verifies
raw invalid UTF-8 inherited output, CR/CRLF captured Cargo JSON, inherited stdin,
whitespace-only library lookup, and stop-after-second-test failure. The native
CLI test uses the same independent expected bytes and command sequence.

Actual validation rebuilt the original tool's four msgq artifacts into a fresh
owned sanitizer target, then ran the native checker on the same source-matched
artifacts. Both completed successfully; artifact SHA-256 hashes remained equal.
Both native C++ peers expose ASan/UBSan instrumentation symbols. Concurrent
nested test output interleaves lines and Cargo's fresh/cached artifact order
varies, so complete output bytes are not claimed equal; test-name occurrence
counts match. No physical device, production IPC namespace, or external service
operation was used.
