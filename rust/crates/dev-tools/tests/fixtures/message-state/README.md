# Independent message-state full CLI reference

The unchanged complete original `check_message_state.py` generated all 11,205
SubMaster frames, actual cereal message bytes, reference state and outcome counts.
Capture environment: Linux x86_64, Python 3.12.15, NumPy 2.5.3, pycapnp 2.1.0 and
zstandard 0.25.0. Full reproduction, commands and logs remain in ignored scratch
`.analysis/scratch/2026-10-16-message-state/`. No expected result comes from the
native messaging implementation.

`provenance.json` retains the immutable original Git blob/SHA-256, exact source
fragments for source/compare/fixtures/main, capture versions and corpus hashes.
These historical fields are separate from `manifest.json`'s live source guard map.
Twenty maintained files still import `source()` and/or `compare()`; do not delete
that live helper or count its file as retired. The native command replaces the
complete checker CLI, not those generic Python helpers or arbitrary runtime input.
After independently validated CLI migration, unused fixture/main scaffolding can
retire while source/compare stay byte-identical and direct old CLI execution must
fail with an explicit native-command redirect. Update the remaining helper's live
pin only, keeping immutable capture provenance and every other runtime/schema/
dependency guard. Do not keep executable dead generator code solely to hash it.

The old comparator differs intentionally from strict diagnostic comparison:
object key order is ignored, float absolute error must be <=1e-12, nonfloat leaves
use Python equality (including bool/int), and signed zero compares equal. NaN
errors fail. The report computes the real maximum error across all frames.
Forty-one complete unchanged-original cases cover accepted reordered keys,
numeric/bool substitutions, signed zero and genuine nonzero 5e-13/exact 1e-12 errors,
above-boundary errors, malformed/UTF-8/BOM/length/shape failures, inherited raw FF
stdout/stderr, failed/missing children and existing output directories.

The checker creates request/expected before launch, interleaving their per-frame
writes; the child owns actual.jsonl. Child stdin/stdout/stderr are inherited, with
no timeout or stream decoding. Both expected and actual files are reread after
child exit using strict UTF-8/universal newlines and strict paired line counts.
Independent child mutation cases preserve reference edits/removal/encoding
failures, including matched edits that the original accepts. A separate invalid
reference/missing actual case preserves opening both files before decoding either. Positive cases retain
exact report bytes; all cases retain exact artifact hashes. Candidate replies are
independent byte-splices of original references, with verified hashes, avoiding
redundant copies of thirty-megabyte traces.

All five original outcome assertions remain required: both all_checks outcomes,
both carState frequency outcomes, and exactly one zero_interval. Keep these gates,
source/schema/package pins and the existing numeric tolerance. Host evidence does
not establish generic ARM behavior or device validation.
The additional three matched truncation/empty/append cases preserve another source
quirk: outcomes stay those computed during full generation, while `updates` counts
the paired files reread after the child (including zero or extra matched lines).

Fresh current-source `state_probe` and native checker runs preserve all four files
and complete stdout/stderr byte-for-byte against the unchanged original, for a
normal path, raw-byte candidate/output and an ASCII symlink to that raw candidate.
The native command runs with `PATH=/nonexistent`; a separate actual controlled peer
proves inherited invalid-UTF-8 stdout/stderr are not decoded. `actual-proof.json`
records concise hashes and results. Raw repository-root equivalence is not claimed
(the existing pycapnp schema loader may reject such roots). Forty-one captured
cases, the five outcome assertion guards and strict focused Clippy pass.
