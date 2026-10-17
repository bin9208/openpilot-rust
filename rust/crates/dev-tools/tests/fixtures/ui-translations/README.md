# Independent dynamic UI translation reference

`original-parser.json.gz` captures the unchanged `_parse_quoted`,
`load_translations` and `PLURAL_SELECTORS` source nodes from
`openpilot/system/ui/lib/multilang.py` using pinned CPython 3.12.15. It contains
47 independently observed PO mutations, all twelve current catalog results and
all 5,733 selector outputs (thirteen languages including fallback, -220..220).
`original-cli.json.gz` captures 58 complete unchanged checker subprocess runs,
including complete original imports and real controlled executable peers in
isolated minimal source layouts. Source identities are embedded; `integrity.json`
records compressed/uncompressed fixture hashes. Capture programs and full logs
are in `.analysis/scratch/2026-10-15-ui-translations/`.

Production expectations are parsed dynamically from the supplied repository's
current PO files by an independent reference state machine. They are not frozen
catalog expectations and do not call or import the candidate UI translation
library. Original parser behavior includes permissive unknown lines/escapes,
blank-only finalization, duplicate-key insertion positions, sparse plurals,
Unicode 15 decimal indices, Python whitespace and universal input newlines.
The checker also preserves languages.json hashability admission and default
English initialization before its sorted catalog traversal. Filesystem names
sort by Python surrogateescape codepoints, including raw-byte names relative to
astral Unicode names. The raw-name source encoder failure leaves the original
empty source receipt after candidate execution.

Whole-checker negative cases preserve source/native file boundaries on failed
PO loads, invalid language initialization, failed candidate execution, strict
UTF-8 failures, malformed/BOM JSON and unequal results. Python bool/number JSON
equality remains supported. Independently measured direct-CLI nesting limits
are context-specific: total 9,986 containers during language initialization,
and 9,997 for candidate output; a deeply nested overwritten duplicate key cannot
bypass these admissions.

Two explicit fail-closed policies supplement the original checker. Retained
multilang.py source semantics are SHA-pinned; a source change requires independent
recapture and native reference review. Catalog edits themselves are not pinned.
Plural expansion permits at most 1,048,576 forms **per entry**, using checked
count arithmetic and fallible vector reservation. This is not a global memory
bound. Huge indices remain accepted when unused by a plural/header entry, up to
the original 4,300 decimal-digit limit; 4,301 digits reject even for leading zeroes.

Only this checker is being retired. The original multilang runtime, other
translation tooling and source consumers remain pending independent conversion.

Verification observed three fail-closed-stub RED tests before implementation.
Four focused tests now pass, including all cases above and exact per-entry
allocation-boundary/source-pin regressions; strict focused Clippy passes.
The actual `translation_trace` example was freshly built with native rendering
disabled (this example only reads catalogs and selects plurals). Complete original
and native checker CLIs both passed all twelve actual catalogs and 5,733 selector
values. Source receipt (296,794 bytes, SHA-256
`1f3d1dec24f561a434ca6ff276db908c28bc7b19cabce1e2dd7286710a5eab3f`)
and candidate receipt (219,593 bytes, SHA-256
`9ed4490f2e0b75ec63389ce6328405b56cfcd7a925e7081e7034cbfe33298aaa`)
matched byte-for-byte across the runs, as did stdout and stderr; no normalization
was needed. Exact commands, source/candidate/checker hashes and outputs are in
scratch `actual-comparison.json` and `actual-{original,native}`.
