# Independent beep integer source comparison

The unmodified original checker generated all 278 inputs, including the 256
leading-byte variants, decimal prefixes/NULs, signed limits, empty bytes and
out-of-range values. Capture invoked the unchanged original Cython Params
binding and C++ `Params::getInt` in 278 real subprocesses before any native
replacement was compiled. There were 33 successful values and 245 SIGABRT
failures. The compressed corpus preserves all raw source stdout/stderr, exit
codes, input bytes, independent outcomes and original complete summary/stdout.
The candidate side alone was replaced by a capture peer forwarding previously
observed source outcomes. It never supplied an independent oracle or executed
the native candidate. The manifest records CPython/binding/source identities.

`failures.json.gz` records 22 original whole-checker controlled-process cases:
source/native exit, malformed/scalar JSON, missing stoi abort diagnostics,
arbitrary stoi abort classification, equality differences, bool/float numeric
equality, forbidden cast logging, spawn/timeout/text-decoding failures and a
blocked summary write. Earlier summaries remain unchanged on every comparison
failure; only all-passing 278-case runs format/write the summary and print PASS.

Native production compares two live processes per input. The source helper
calls unchanged C++ Params::getInt on a private on-disk SoundVolumeAdjust value.
Empty strings return zero; other inputs reach original std::stoi. Exceptions
remain uncaught, retaining real SIGABRT/libstdc++ diagnostics. The helper only
sets core/dumpability limits, exposes the JSON result and supplies the established
Params reference logging sink. It does not contain candidate integer policy.
The existing native Params reference builder provides schema staging and source
build commands; the integer helper compiles against the same Params/util sources.
A receipt binds the source executable and original sources before use.
The builder refuses an existing output directory and writes its receipt only
after all three compiler stages, executable reading and source hashes succeed.
Pending tests inject failure at every stage and preserve foreign prior receipts.
Eleven original CLI captures map only `--binding` to `--reference`; the checker
and source-builder parser tests require help/errors to leave no artifacts.

CLI migration is explicit: `--reference` selects the native source executable;
the former `--binding` Cython module is no longer used by this checker. The
report's former `python` lane is now `source-reference`; PASS now says
`original C++ Params/native` instead of `real Cython/native`. These are explicit
provenance deltas: the live native checker does not execute Cython/Python.
Tests adapt exactly those two labels in original captures and preserve all
other report bytes, values, diagnostics and failure order. Source-build output
must be a fresh directory, so failed builds cannot reuse old success receipts.
Original beep_source.py, beep_fixtures.py, original_params_binding.py and the
binding builder remain shared dependencies of other unconverted checks and must
not be removed with this fixed checker. Only check_beepd_integer.py is eligible
for retirement after real original/native helper parity, fault tests and native
PATH-none execution are verified. No full beep/reference retirement is claimed.

Validation on 2026-10-18: all six checker/builder tests and both CI route tests
pass. The live helper matches all 278 independent Cython captures byte-for-byte
for exit, stdout, stderr and classified outcome, including 245 real fatal aborts.
The actual native beep_trace candidate passes all 278 inputs. After retiring the
Python checker, PATH=/nonexistent execution produces identical stdout and summary.
Strict Clippy passes with the production polygon-reference feature enabled.
This removes only this checker; the shared source/binding dependencies above remain.

After building the source helper, run from repository root:

```
/home/fma1/rust/openpilot-rust/.venv/bin/python -B \
  .analysis/scratch/2026-10-18-beep-integer/prove_reference.py \
  --reference .analysis/scratch/2026-10-18-beep-integer/native-reference/beep-integer-reference \
  --output .analysis/scratch/2026-10-18-beep-integer/reference-proof.json
```

This scratch-only script compares every native source exit/stdout/stderr byte
and classified outcome to the real independent Cython captures, verifies corpus,
executable and source receipt hashes, and records all 278 comparisons. It does
not build, invoke the candidate or become a production Python wrapper. The
helper output directory above must be fresh when passed to
`build-beep-integer-reference --output`.
