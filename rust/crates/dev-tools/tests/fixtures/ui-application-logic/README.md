Complete fixed-input original GuiApplication method captures: 112 ordered FPS
samples and 19 ordered stack operations. Original checkers execute the unchanged
method bodies with their original clock/exit/widget seams. No expected output
comes from a candidate implementation. CPython 3.12.15/interpreter and source
hashes are in provenance.json; compressed/plain SHA256 and complete lengths are
in integrity.json. stdin assets preserve compact child input separately from the
pretty input.json artifacts. Capture drivers remain scratch-only.

failures.json records42 actual original subprocess cases, including stale files,
strict stdout decoding, inherited raw FPS stderr, strict captured Stack stderr,
JSON/BOM/scalar/NaN/surrogate handling and Python bool/number equality. cli.json
adds18 original argument/help cases. depth.json records decoder9997 admission
and FPS serializer996 admission: depth997 serialization fails before native.json
creation, retaining input/source and any preexisting native receipt. Whole-case
checks preserve partial file bytes and artifact ordering rather than normalizing
away differences.

Only these fixed-input comparison CLIs are replaced. Generic GuiApplication and
other source consumers remain pending. The original Probe/clock/exit seams are
retained in the independent captures; they do not establish real rendering or
recording lifecycle behavior. Source-byte changes require independent recapture,
a deliberate fail-closed change from the original dynamic source evaluation.
No Python interpreter is invoked by the native checker. Evidence is Linux
x86_64; no ARM original-output equivalence is claimed and no new host rejection
is introduced. The original CLI has only --binary and --output, so no custom
input interface or generic runtime capability is silently removed.

Five focused tests and strict Clippy pass after a semantic fail-closed-stub RED.
Fresh actual fps_trace/stack_trace candidates pass both original and native
checkers, with all8 artifacts and stdout byte-identical (actual-comparison.json).
raw-comparison.json proves original/native non-UTF8 physical root, candidate,
output and inherited environment behavior, including raw FF stderr passing
through FPS unchanged. Full capture programs/logs are in scratch
.analysis/scratch/2026-10-15-ui-application-logic/.
