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
