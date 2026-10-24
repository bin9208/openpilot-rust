# Checkout tools independent original captures

The unchanged seven checkout Python tools and retained
`openpilot/system/manager/update_status.py` were captured before any Rust checker
implementation. `manifest.json` binds all eight source hashes and each synthetic
fixture. The regression test pins the manifest digest, so changing expected bytes
and recomputing its internal hashes does not silently rebaseline the tests.

Fresh full original checker: **nine suites PASS, 6.93 seconds**, using current-source
`checkout_trace`, `git_fixture`, `captured_trace`, `handshake_fixture`, and
`openpilot-process-child`. Parent built and froze those five ELFs; exact ELF hashes
are in `full-original.json`. Full source/build receipt, both implementations'
transcripts, raw child/descriptor/env/argv observations and cleanup are ignored
evidence under `.analysis/scratch/2026-10-18-checkout-tools/`:
`baseline-build-receipt.json`, `fresh-original-receipt.json`, `fresh-original.log`
and `fresh-original/`. The run used the original strict assertions unmodified.

The full baseline covers 50 metadata cases; real SHA-1/SHA-256/separate Git-dir;
17 executable cases; six packaged updates; 44 Git-backed state update rows and
actual invocation counts; 11 native helper probes; and six independent actual
Python `subprocess.call` inherited-stdio comparisons. Helper-only tests are native
infrastructure contracts in the original script, not a fabricated Python oracle.
`full-original.json` keeps only the seven **Python source-side** logical results,
the source-side inherited outcomes, and helper names. Candidate-side outputs never
become policy expectations. Raw timing/temporary-process identities remain in the
original ignored evidence; the existing timing inequalities must stay unchanged.

`metadata-policy-original.json` runs the unchanged metadata case function with
real filesystem reads until the first real-Git branch: 48 rows, including the
43 direct byte-parser rows. Last two Git-selected metadata cases are covered by
the full baseline. `state-policy-original.json` executes the unchanged original
state function and `UpdateStatus` with an injected checkout reader, recording all
44 update rows, raw f64 bits, captured identities, returns and read counts. This
isolates policy and does not replace the full actual Git/process baseline.

`cli-original.json` records 12 actual original help/admission failures without
candidate execution. Only the repository absolute prefix in output bytes is
replaced with `$ROOT`; original unmodified bytes remain in ignored
`policy-original-raw/`. Argument quoting, line breaks, exit codes, ordering and
output-directory creation observations remain exact. Full successful CLI/group and all-group partial-summary/error phases now pass
against the frozen native CLI, including twelve independent failure phases.

Capture runners and raw receipts remain ignored in that scratch directory.
Native checker/reference conversion passed ten focused tests and strict Clippy,
full nine-stage original comparisons, twelve original failure phases and raw-Unix
path/environment receipt controls, both before and after retiring exactly seven
Python tools. Both final 426-exec traces contain zero interpreter attempts.
The runtime `update_status.py` remains pinned and mandatory. These host checks
do not establish device acceptance or complete repository-wide Python retirement.
