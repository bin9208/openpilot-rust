# Independent widget and navigation checker corpus

The unchanged whole `check_ui_widgets.py` and `check_ui_navigation.py` commands,
Python 3.12.15 and original source modules generated these fixed CLI corpora.
Ignored reproduction: `.analysis/scratch/2026-10-15-ui-headless/capture_v2.py`.
The controlled candidate consumes the original fixture on stdin and emits explicit
responses; no expected values come from a Rust widget/navigation implementation.

`original-cases.json` records 44 complete original command results, reply hashes,
stdin hashes, exact artifact hashes and original stdout. Gzip files have mtime zero.
The widget corpus contains 800 seeded input frames; navigation contains 480 frames.
Runtime source modules remain pending. A fixed-corpus checker is not a generic UI
runtime replacement. Source pins and the navigation NumPy locked dependency record
must fail closed on drift and require independent original recapture.

Preserve the comparators separately: widgets use Python list/dictionary equality,
including numeric/boolean equality, and reject unknown object keys. Navigation
iterates equal-length traces and only expected keys, permits unknown keys, and
requires strict absolute difference below 1e-6 for every expected numeric/boolean
field. Exact-boundary, NaN/infinity, missing/extra rows and keys are captured.

Both commands create input/source JSON before executing the candidate, capture both
streams with strict UTF-8/universal-newline decoding, and reject nonzero status
before native JSON is written. Successful subprocess decoding writes native JSON
before JSON parse/comparison. Neither checker preserves captured stderr as a file.
The original execution has no timeout or stream limit; this port must not silently
introduce either. Source/dependency integrity checks are explicit additional guards.

Capture environment: Linux x86_64, Python 3.12.15, NumPy 2.5.3. This is an
independent host reference, not proof of generic ARM floating-point equivalence.
`oracle_sources` separately pins the two unchanged capture drivers as provenance;
those drivers need not remain installed to run a verified native checker.

Fresh host validation: rebuilt `widget_trace` and `navigation_trace` with
`openpilot-ui-framework --no-default-features`; complete original and native
checker commands both pass, all three files per family and stdout/stderr are
byte-identical without normalization. Native runs use `PATH=/nonexistent`.
Four additional complete original/native pairs cover raw non-UTF-8 candidate and
output paths and ASCII symlinks pointing to those candidates. `actual-proof.json`
records hashes and results; full commands/logs remain in ignored scratch. Focused
44-case fixture tests and source/dependency mutation tests pass, as does strict
Clippy for the library, test and both bins. These host checks do not establish ARM
or generic runtime equivalence.
