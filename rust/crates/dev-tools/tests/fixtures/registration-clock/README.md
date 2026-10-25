# Registration clock checker source controls

The unchanged 62-line `rust/tools/check_registration_clock.py` at SHA-256
`4641d2e73a12c476a67b418210148a54e178129b94abf65b968a107f302b1920`
produced these controls before the native implementation. The checker imports
only Python standard-library modules. `policy-original.json` contains all 13
original tuples and 72 additional inputs executed through the unchanged AST
calendar loop, including microsecond admission and year/expiry boundaries.
The native source policy computes Gregorian year limits independently; it does
not import the registration runtime or read these fixtures at execution time.

`cli-original.json` preserves nine actual original subprocess results as bytes.
Tests change only the executable spelling from `check_registration_clock.py`
to `check-registration-clock`. `failures-original.json` preserves eight actual
child/output failure and admission cases: missing/nonzero/empty/echo/malformed
children, unavailable output parent, output directory and Python bool/float/int
container equality. The cat/printf/true/false processes are protocol boundary
fixtures. Their outputs do not define the source calendar policy.

The complete original checker also passed against a freshly built real
`registration_clock` ELF, with an env runner prefix and non-UTF-8 hardlink and
output names. Its capture ELF SHA-256 was
`77ab0e6745cb64af10d56ff09f3a9a530fe766b7888a41fa8d7fea67af093728`.
Private capture scripts, raw original report/stdout/stderr and the five-input
build receipt are under
`.analysis/scratch/2026-10-19-registration-clock-tools/`. The manifest records
script and corpus hashes; no private absolute paths or ELF payloads are checked
in here. A fresh candidate may have a different ELF identity; it must still
satisfy every independent expected result.

The native controller preserves original report fields, insertion order,
indentation, no trailing newline, lossless Unix argv, no automatic output-parent
creation and writing an `equal: false` report before returning a mismatch.
Pre-publication child/parse failures do not create a new report. Unlike the
original, a prior regular `equal: true` report is invalidated before source
admission or child startup; arbitrary failure artifacts remain intact.
Native additions are owned-process cancellation, a 60-second child bound and
removal of this invocation's report if cancellation is observed during its final
write. The latter uses inode identity and does not unlink a replaced output.
Only genuine absence of the retired checker is accepted; changed or dangling
reintroductions fail. Source presence and hashes are checked on both sides of
execution and publication. Live and historical identities stay separate. A
nonblocking regular-FD fingerprint retains the input inode across execution;
post-execution identity/hash changes reject success. Fingerprint errors remain
deferred until the original post-parse hash phase, preserving real spawn and
decoder failures without blocking on FIFO inputs.

Actual process-barrier regressions reproduced executable/source substitution
and stale-PASS retention before repair. The repaired sixteen-test gate and
strict Clippy passed. `binding-manifest.json` pins unchanged-original
inline-runner/FIFO controls and `source-original.txt`, used only as copied-root
source-guard test data, never interpreted or imported. The original three
expectation corpora remain byte-for-byte unchanged.

The same frozen checker/probe passed both before and after Python deletion:
the entire original 13-case report matched byte-for-byte, CLI/failure/raw-path/
runner controls passed, and execve/execveat traces contained only the checker
and probe, with no interpreter attempts. Binding and stale-PASS negatives also
passed. Source guards rejected changed, dangling and FIFO entries; matching
historical bytes were admitted only in private fixture roots. The historical
`.txt` is nonexecutable and was never restored to the live tool path. These are
bounded before/after identity checks, not an atomic execution snapshot against
an ABA substitution.

`REGISTRATION_CLOCK_BINARY` must name the fresh absolute probe path; there is no
optional environment skip. Tests exercise the registered native executable,
including a real Linux FIFO final-write SIGINT boundary. Full before/after
receipts and listed source/control/ELF seals are indexed by private
`FINAL-PROOF.json`. Fresh post-deletion tests and strict Clippy also pass.
