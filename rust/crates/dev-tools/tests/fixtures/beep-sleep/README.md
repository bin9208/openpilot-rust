# Independent beep sleep corpus

The unmodified `check_beepd_sleep.py` supplies all 27 fixed inputs. CPython
3.12.15 `_PyTime_FromSecondsObject` with `_PyTime_ROUND_TIMEOUT` independently
produced expected nanoseconds/errors before any native candidate was used.
Only the eight original-rejected values reached guarded real `time.sleep`;
no accepted duration was slept. The manifest pins the original source and
complete compressed corpus, recording interpreter provenance and all eight
rejected bit patterns. Native execution has no Python dependency or sleep.

Thirty-five standalone original CLI captures cover exact report/stdout bytes,
Python JSON membership and equality, strict cardinality, malformed text/UTF-8,
Unicode line separators, decimal integer and JSON recursion boundaries.
Error rows preserve the original error-key-only comparison, including accepted
string/list membership and deliberately differing error names. Existing
`native.stdout`, `summary.json` and unrelated `inputs.json` start as `keep`.
The source parses, checks row count and performs membership before any write;
then it writes native.stdout, formats/writes summary, prints and asserts.
Two filesystem fault captures verify this write order. Seven original CLI
captures cover side-effect-free argparse help and errors.

The standalone original accepts 991 nested arrays beneath a candidate error
key; 992 fails indented summary formatting after native.stdout but before
opening summary.json. Decoder-overflow cases fail before either artifact.
At the decoder boundary, 9,996 arrays beneath an error key decode and reach the
native.stdout write; 9,997 arrays fail before touching either artifact. The
enclosing object counts as one container. These failure expectations were captured in real subprocesses to retain the
original main's call depth. No candidate policy or native output supplied the
oracle. Reproduction script and concise capture log are retained in the task's
ignored `.analysis/scratch/2026-10-18-beep-sleep/` directory.
