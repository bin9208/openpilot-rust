# Independent diagnostics/model-timing checker references

Captured on Linux x86_64 with Python 3.12.15 and pytest 9.0.2, by executing both
unchanged complete original CLI drivers and their unchanged runtime source. No
expected values come from the native candidate. Reproduction scripts, complete
commands, responses and logs remain under ignored scratch
`.analysis/scratch/2026-10-16-diagnostics-checkers/`.

The full fixed CLI corpora comprise 18,314 runtime-diagnostics steps (6,000
aggregation, 12,011 rounding and 303 integer/overflow), with 12,385 emissions, and
2,053 model-timing steps with 536 aggregates. The original modeld record expression,
backend-name mapping, first four interval boundaries and thread CPU clock contract
are retained in the timing corpus. Source hashes and the complete locked pytest
record fail closed on drift and require independent recapture. Generic runtime
helpers remain pending; these frozen test inputs do not replace arbitrary runtime
behavior or establish ARM equivalence.

`original-cases.json` records 92 whole-driver cases and 16 direct original comparator
characterizations. Comparison requires identical JSON types, dictionary insertion
order, list lengths, exact integer/string/bool values and float64 bits. Any two NaNs
compare equal; signed zero differs. Tests cover one-ULP, integer/float and bool/int
changes, reordered/missing/extra data, staged second/third-scene failures, UTF-8,
newlines, failed/spawned children and existing output directories. Artifact hashes
are checked exactly. Compressed corpora and candidate responses are content-hashed.

Preserve differing artifact phases. Diagnostics parses candidate JSONL before any
scene artifacts, then writes input, re-encodes the parsed candidate list, writes
actual/expected, and compares. Timing requires a new output directory, writes input
before launch, and writes captured native output and source output before JSON
parse/comparison. Failed diagnostic children leave no scene artifacts; failed timing
children leave input. Earlier successful scenes remain after later failures.

Whole pinned CLI measurements admit JSON container depth 9,997 and reject 9,998.
Diagnostics' default C JSON encoder admits total depth 9,997; a parsed line of depth
9,997 gains an outer list and fails encoding after input is written. Timing keeps
its three files on either decoding/type failure because it does not re-encode the
candidate. These limits are measured original behavior, not arbitrary added caps.
Child execution, stdout and stderr retain the original unbounded behavior; both
streams use strict UTF-8 and universal-newline conversion. JSONL splitting follows
Python `str.splitlines`, including Unicode separators.

The closure also includes communication snapshots: the unchanged full original
`check_communication_snapshot.py` imports the same exact comparator, and exercises
11,205 SubMaster updates from `check_message_state.py` with actual cereal messages.
Its independently captured source references retain service ordering, duplicates,
unknown services, clock offsets, simulation/poll/frequency/ignore options and the
source's zero-interval handling. Additional pins cover the fixture/source helper,
actual messaging code, MovingAverage, service catalog, all imported cereal/car
schemas and complete locked NumPy/pycapnp/zstandard records. These runtime helpers
remain explicit pending dependencies. Communication parses JSONL before creating
scene files, then writes input, expected and actual, in that order. A depth9,997
candidate therefore leaves input+expected when outer-list encoding fails; depth
9,998 leaves no files. Existing output directories are accepted. Its 22 whole
original positive/negative cases are included in the 92-case fixture.
Communication capture dependencies were NumPy 2.5.3, pycapnp 2.1.0 and zstandard
0.25.0, matching the pinned lock records. `check_message_state.py` and generic
runtime helpers remain in the repository. Migrating all three checkers is necessary
before deleting `check_runtime_diagnostics.py`, because Communication imports its
comparator in the original implementation.

Fresh actual host proof: rebuilt `openpilot-logging` examples `diagnostics_trace`
and `communication_trace`, plus `openpilot-driving-modeld` example
`model_timing_trace`. Complete unchanged original and native checker commands all
exit zero; all 18 files across the three families and stdout/stderr are byte-identical
without normalization. All native runs use `PATH=/nonexistent`. Six additional full
original/native pairs cover raw-byte candidate/output paths and ASCII symlinks to
raw candidates, again exact. This does not claim physical raw-byte repository-root
equivalence: pycapnp's source schema-loader limitation remains separate. Hashes and
compact results are in `actual-proof.json`; full commands and logs remain in ignored
scratch. All four focused tests (92 whole-original cases, strict scalar behavior,
source/schema/dependency drift) and strict library/test/three-bin Clippy pass.
