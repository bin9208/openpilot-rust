# Original identifier checker contract

`original.json` was captured from unchanged `rust/tools/check_logger_identifier.py`
at `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, verified byte-for-byte before capture.
The source SHA-256 is recorded. A controlled subprocess peer returned a fixed
valid identifier and stderr, without modifying the counter files. The original
checker performed all 80 filesystem preparations, permission changes/restoration,
identifier parsing, raw log retention, binary hashing, and per-case JSON writing.

The fixture records both keys, all ten exact byte inputs (including embedded NUL),
all four I/O modes, argv/Params environment, observed modes, outputs, results, and
artifacts. File/directory creation modes inherit the caller's umask. Native tests
use independently created control paths to account for a different test umask;
the explicitly forced 0, 0500, 0700, and 0600 modes are checked exactly.

Three additional original failure captures cover nonzero status with raw NUL/CRLF
output, malformed identifier, and invalid UTF-8. They verify that access is restored
and stdout/stderr/invocation records survive rejection, without a success result.

The production native checker continues to execute the independently built,
unchanged C++ identifier oracle beside the Rust candidate for every case. This
controlled checker fixture does not replace that live behavioral comparison.

Additional independent captures cover output-path resolution through missing
components, `..`, existing and dangling symlinks, a symlink loop, and a regular
file prefix. A mismatching source/native identifier capture verifies that both
per-case result records survive while the overall success report is absent.

The native subprocess runner adds a 30-second execution bound to each identifier
invocation; the original checker had no timeout. This is a bounded-execution
validation guard. All 160 completed invocations in the live 80-case comparison
retain the original results and failure effects; no completed normal result is
changed by the bound.
