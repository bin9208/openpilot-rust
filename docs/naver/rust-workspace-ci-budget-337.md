# Complete workspace CI budget (#337)

Issue: https://github.com/bin9208/openpilot-rust/issues/337

Exact merged dev `a271bdf5312b7c2fd214e310394c6465ce68772d` failed its separate
post-merge gate: [workspace job116059218723](https://github.com/bin9208/openpilot-rust/actions/runs/38624236116/job/116059218723)
started2026-10-14 19:30:28UTC and ended cancelled20:45:54UTC,75m26s later.
No successor dev push cancelled it. The configured job budget was75minutes.
The timing, configuration and active compilation strongly identify that deadline;
the connector cannot read its two check annotations, so this remains an inference.

All preceding workspace steps succeeded, including strict Clippy, full tests and
original/native comparisons. The final release build started20:17:34UTC and was
still compiling when the runner emitted `The operation was canceled` at20:45:50.
Cleanup killed cargo and two rustc processes. There is no compiler error in this
final build log. The release build and later sample/artifact steps therefore did
not complete; the cancelled job is not counted as success.

Only the workspace job timeout changes from75 to120minutes. Every command,
target, assertion, source pin, disk guard and artifact requirement is retained.
Two build workers and incremental-off remain unchanged. This adds time for the
existing complete build without skipping any gate. Fresh exact PR HEAD and
separate merged-dev PUSH checks must pass. This is distinct from Card#335 and
does not establish AGNOS/device startup, driving or CPU savings.

Validation: existing CI policy tests, native mapped-doc validation and diff
checks; fresh complete cloud execution remains required.
Docs-Not-Needed: internal CI execution budget only, without user behavior changes.
