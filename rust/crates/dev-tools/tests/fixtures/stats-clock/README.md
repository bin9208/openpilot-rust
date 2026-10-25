# Independent stats clock corpus

Captured from the unchanged `check_stats_clock.py` main on CPython 3.12.15,
before any native candidate was run. Its original fixed-seed generator supplies
all 5,048 calendar inputs and 5,009 monotonic inputs. Expected calendar integers
come directly from `datetime.fromtimestamp(..., UTC)` with the original
microsecond truncation and double rounding; expected monotonic bit patterns come
from the installed CPython `_PyTime_AsSecondsDouble` C API. The two unsupported
calendar boundaries retain their original ValueError classifications.

The manifest records complete CPython version, implementation, byte order, source
checker SHA256, both corpus hashes, expanded size and all three case counts.
CPython itself is an external oracle, not a retained runtime dependency of this
fixed-input checker. No native candidate output generated these expectations.

`failures.json` captures the original main with 25 controlled process result
sequences, including both mismatches, cardinality, malformed integers, decoding,
nonzero exits, boundary acceptance, Unicode line breaks and integer spellings.
Every failure records process call count, exact stdout and final report bytes;
the output begins as `keep` so premature truncation is detectable.
`integers.json` separately records 38 Python int admissions, including Unicode
digits, signs, whitespace, underscores and the default 4,300-digit limit.

## Native validation (2026-10-18)

The registered integration test fails against the unimplemented production stub
and passes after implementation: four focused tests cover all captured outcomes,
integer admission, source drift, missing report parents and the original
non-argparse CLI. Focused strict Clippy passes.

The dedicated #296 cache builds the actual `openpilot-statsd` `stats_clock`
example from this worktree. Original CPython and native checkers produce identical
598-byte report files and identical stdout, with zero calendar or monotonic
differences. Repeating with `PATH=/nonexistent` and a source root containing no
Python checker produces the same bytes. The example SHA256 for this proof is
`738b8dec4359d96c5f8fc4185e3e6ae26f0faef7c2320e6a587775c94a5cd537`.
The concise reproduction logs and `actual-proof.json` remain under the ignored
`2026-10-18-stats-clock` scratch directory. These are host clock/tool checks,
not device or driving acceptance.

The checker now normalizes decimal strings locally, using the existing
CPython-pinned `argparse_scan` digit table. It does not import carrot-navi,
logmessaged or cereal merely to parse arbitrary-width integers; the default
fast-check build therefore retains its original schema-free dependency boundary.
`unicode-integers.json` independently enumerates all 680 Unicode 15.0 decimal
digits and 29 whitespace classifications from CPython 3.12.15. A unit check
compares every Unicode scalar against the captured digit set and exercises
signed/underscored normalization and whitespace admission.

After removing the daemon dependency, the refreshed gates pass five clock
integration tests and the exhaustive Unicode unit. The rebuilt lightweight
checker again matches the original 598-byte report and stdout against the same
actual example, using `PATH=/nonexistent` and the real repository root after the
original Python checker was deleted. `light-native-proof.json` records hashes.
