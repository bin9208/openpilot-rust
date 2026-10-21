# Native calibration checker CLI parity (#436)

Issue: https://github.com/bin9208/openpilot-rust/issues/436

At source `b43ac10d78c20029c86028e96811678f8cff5a1d`,
[run39521903482/job118967468077](https://github.com/bin9208/openpilot-rust/actions/runs/39521903482/job/118967468077)
passed all25 original/native getFloat outcomes, generated C++, reports, stdout
and the paired positive test. Ten integration tests passed; two CLI/environment
controls failed. This was not complete retirement acceptance.

The assertion compares native then original: `-hfoo` produced native exit2 and
original exit0. Calibration's extra argument rewrite incorrectly rejected the
original short-help cluster. The rewrite is removed. The shared scanner keeps
normal help action ordering while rejecting explicit tails such as `-h-h` and
`-hh=foo`; the expanded cloud matrix checks those cases against the actual
original interpreter. Unknown tails such as `-hfoo` still execute help.

With PATH unset in a private cwd, the original exits1 because GCC cannot find
`cc1plus`; native previously succeeded. The native driver was bound by absolute
path, which also changed its argv[0] and therefore GCC helper lookup. CommandSpec
now permits a lossless child argv[0] override while retaining its independently
selected program path. Calibration executes the hash-bound absolute compiler
with original argv[0] `g++` and records the program and actual child argv
separately. Other callers retain the default behavior.

The PATH control compares the original/native exit, stdout and success-receipt
behavior instead of assuming an unset PATH succeeds on every GCC installation.
An explicitly empty PATH must still fail in the private cwd. A native process
unit exercises non-UTF8 argv[0] while confirming it does not select a different
executable. Parent source review is complete; fresh cloud compilation, the full
CLI matrix,25-case pair, cancellation controls and strict Clippy remain pending.
The original Python checker remains present. No device result is claimed.


## Repaired exact-source cloud result (2026-10-21)

Source `60e3711e09d6b45b66a3f5ac30d79d6521f86e0c` passed
[run39523577786/job118972931515](https://github.com/bin9208/openpilot-rust/actions/runs/39523577786/job/118972931515).
Artifact12312123228 is36,862bytes, archive SHA256
`fbb136aabb348e8efd312fea55e84110b93588294bccea785184c3958420913f`.
All25 fresh original/native parsed reports and stdout agree; generated C++
source and separately compiled reference ELF bytes are identical. Report file
serialization differs, so raw report bytes are not claimed identical.
Twelve CLI/control tests include clustered help and unset/empty PATH parity;
31shared process tests, one scanner test, four library tests and strict Clippy
passed. Native provenance separately records `/usr/bin/g++` and child argv0`g++`.
The original/native PATH exit/stdout/receipt assertions passed, but individual
unset-PATH codes/stderr were not retained; the next retirement proof should
retain those raw observations. Python remains until that proof is accepted.
This result does not close whole-project or device acceptance.


The separate finalization finding is tracked in #437. Source-specific opens
now reject symlinks without changing compiler-alias lookup. The historical
checker's parent is retained as a real directory descriptor and compared by
identity through final receipt commit. Admitted source/input/executable handles,
presence/absence, cancellation and owned receipt guards remain live across
handler teardown. A parent replacement, reappearing/changed/nonregular source
or foreign receipt cannot be accepted at that final check. This is not an
atomic transaction against arbitrary changes after the final check.
Baseline-compatible test-only grafts on60e3711e must compile and fail at the
specific stale-publication assertions, then current regression/25-row retirement
and raw CLI/PATH tests must pass. That fresh cloud proof remains pending; the
Python checker is still present. No numerical tolerance was changed.
