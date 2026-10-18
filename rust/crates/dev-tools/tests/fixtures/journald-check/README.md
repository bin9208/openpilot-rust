# Independent journald checker captures

All source/policy inputs are synthetic; actual IPC/process raw evidence remains
ignored. No device data, credentials or binary artifacts belong in this directory.

- policy-original.json binds ten original checker/helper/runtime source hashes.
  Before any native module existed, unchanged valid_records() produced138 records
  and unchanged entry() produced155 success/error outputs including17 edge inputs.
  Large Python integers, seeded384-bit draws, binary64 rounding, insertion order,
  nonfinite values, surrogate text and5000 nested arrays remain exact.
- fixture-original.json records12 actual unchanged child processes: fragmented
  commands, binary bytes, EOF, pending EOF, close, explicit exit, malformed
  commands, permissive base64, closed-output write and real SIGTERM. Dynamic
  parent/child IDs and monotonic fields were checked before normalization.
- base64-original.json adds nine actual original helper processes covering
  leading/extra/trailing padding, ignored ASCII junk and non-ASCII rejection.
  Five disagreements were observed with the initial frozen native helper before
  its generic permissive decoder was repaired.
- readiness-original.json preserves two real original early-child-failure
  captures. Native regressions check both daemon and collector exit without
  reaping away process ownership or waiting for the readiness deadline.
- process-original.json records the unchanged full original process matrix,
  omitting only dynamic IDs, elapsed times and timestamps from this summary.

The unchanged full original checker was also freshly executed, confirming155
messages,13 recoverable parse errors,19 fatal cases and four lifecycle cases.
All three unchanged original signal regressions passed, including SIGTERM during
trace append and blocked read/write. Captures and commands are indexed in ignored
.analysis/scratch/2026-10-18-journald-tools/.

The production native path passed 60 tests, including four actual process/IPC
ownership regressions, all helper cases, every source-pin mutation, allowed
retirement absence and the complete independent reference matrix. Scoped strict
Clippy passed. A fresh current-source journald-rs and both native tools were
frozen with source and executable SHA-256 receipts before the final comparison.
Both the independent reference and candidate passed155 messages,13 recoverable
errors,13 disk records,19 fatal cases and four lifecycle cases. Their155 Android
message rows exactly equal the fresh unchanged original rows, including message
strings. The final native-only PATH contains only git;147 traced executions
contain no Python. No traced children or owned IPC endpoints/namespaces remained.
Exact receipts, reports and execution trace are retained in the ignored scratch
directory as frozen-current/receipt.json, native-only/report.json,
native-only-summary.json and native-only.execve.

Only the five owned journald validation tools may be absent after retirement.
The shared logmessaged_reference.py and every other pinned runtime dependency
remain required; changed or reintroduced retired sources are rejected.

The oracle policy uses generic independent JSON/Unicode/float/MT19937 helpers.
It never calls the candidate journald decoder/packet builder and never loads
captured expected rows to implement runtime behavior. Native child signal code
records on its owning loop; its signal handler only wakes a self-pipe.
