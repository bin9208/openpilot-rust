# Independent original emoji checker fixtures

The fixed seven-input `check-ui-emoji` command uses PNGs emitted by the unchanged
`emoji_tex` from `openpilot/system/ui/lib/emoji.py`, Pillow 12.3.0 and the pinned
NotoColorEmoji font. It does not use the candidate Rust rasterizer to generate
expected pixels. `original.json` pins source/font/image identities and the two
reference dependency versions. Changes require independent recapture. The generic
source emoji parser/rasterizer remains a separate, pending Python dependency;
these seven fixed inputs do not replace that callable implementation.

`original-cases.json` and `provenance.json` retain the original 2026-10-15 record
of 21 complete checker executions. `broad/` contains the corresponding controlled
candidate responses, copied only after checking every original artifact hash.
The peer read source PNGs written by the original checker, never candidate pixels.
TGA, TIFF, lossless WebP, RGBA16 PNG, reencoding, hidden RGB, grayscale/palette
broadcasting, missing/malformed/truncated files and early/late child failures
remain covered. Content decoding preserves the original array mode rather than
normalizing the candidate to RGBA or limiting it to PNG.

`original.json` additionally records 12 fresh original scenarios and partial
artifact hashes, including source-file overwrite, report-directory failure and
late child failure. The native checker compares the immutable original pixels
retained in memory even if the candidate overwrites the diagnostic source PNG.
It preserves seven invocations before pixel-difference failure, report field
order, UTF-8 JSON without a trailing newline, and earlier failure artifacts.
CLI argument, source/font/dependency drift and actual stdin/stdout/stderr
inheritance tests supplement these independent cases.

These are host validation-tool comparisons, not device or driving acceptance.
Original decoder captures and scripts remain private scratch evidence under
`.analysis/scratch/2026-10-15-ui-emoji/` and `2026-10-18-emoji-check/`.
