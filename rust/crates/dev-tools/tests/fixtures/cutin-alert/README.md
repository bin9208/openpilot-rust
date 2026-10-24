# Independent fixed cut-in audio-alert checker corpus

The unmodified original `check_cutin_alerts.py` generated 5,786 synthetic frames
and evaluated the retained original `CutinAlertTracker` and
`promoted_cutin_candidates` before capture at its subprocess boundary. Every
selected candidate, previous-state candidate and alert boolean is preserved in
the original JSON source lines; float inputs remain IEEE754 u64 bit patterns.
The corpus includes signed identity limits, exact/adjacent thresholds, repeated
candidates, promotions, resets, disabled updates, NaN/Infinity/signed zero and
the complete deterministic 5,000-frame randomized suffix. No candidate supplied
an oracle. The manifest pins original checker/runtime source and interpreter,
compressed/uncompressed SHA256 and complete byte lengths.

Seventeen original-main controlled-process captures retain exact stdout and
artifact size/SHA256, including mismatches, short/extra/empty output, malformed
JSON/BOM/NUL, failure status, timeout, UTF-8 errors, diagnostic stderr, universal
and Unicode line separators, numeric/bool JSON equality and missing binary.
Six captured CLI cases retain argparse statuses and side-effect-free help/errors.
The native gate keeps the source's 20-second timeout, exist_ok=False and its
specific output ordering: input/source lines before spawn, native output files
before status/JSON/equality checks, original binary-path hash after comparison,
then exact success report formatting. No generalized Python wrapper or runtime
algorithm is introduced. Runtime source drift requires independent recapture.

This converts only an existing validation script. Detection, lead-selection,
runtime cut-in audio policy and NAS replay code are unchanged; no device or
vehicle acceptance is claimed. The single caller is the selfdrive-runtime
source-comparison workflow. Registration, workflow migration and original deletion
are complete. The actual original/native check of 5,786 frames produces identical
input/source/native/stderr/report files and stdout. After original deletion,
PATH=/nonexistent repeats those bytes. All 28 affected checker/routing/policy
tests and scoped strict Clippy pass; the three checker and three routing tests
also pass after deletion. Reproducible captures and parent-gate.log are in the
ignored `.analysis/scratch/2026-10-18-cutin-alert/` directory.
