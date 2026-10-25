These original control-tool receipts were captured by executing the Git source
identified in `provenance.json`, with an owned shell peer that reads stdin and
prints an empty JSON array. Four cases put an invalid UTF-8 byte in binary,
source, output, or a resolved symlink target. `BASE` normalizes only the owned
temporary ancestor. Receipts and printed output retain Python's exact JSON
surrogateescape spelling. The peer bytes, requests, receipts, stdout/stderr
files and successful status are checked by `unix_cli_paths.rs`.

The additional actual archive audit used the previously verified pinned
charset-norm archive via owned symlinks, testing invalid UTF-8 archive/output
paths. Both original and native commands verified the same 28 vendor files and
produced byte-identical reports. That downloaded archive is not duplicated in
Git; the existing 37 original vendor cases remain the repeatable unit evidence.
The CLI regression also checks that raw archive/output arguments reach ordinary
file I/O errors rather than panicking during Unicode argument conversion.

Before repair the five raw CLI cases panicked with exit 101; the ASCII alias
case instead created an empty output and failed with exit 1 when its resolved
binary path was not Unicode. All six actual original/native pairs now exit 0
with equal stdout, receipts and artifact sets. This bounded fix does not claim
byte-path coverage for other retired command families.
