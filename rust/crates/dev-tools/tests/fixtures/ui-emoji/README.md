# Pending emoji checker decoder equivalence

`rust/tools/check_ui_emoji.py` remains an explicit Python dependency. Its seven fixed
texts admit an independently captured source raster baseline, but PNG-only candidate
decoding is not equivalent to its Pillow/NumPy comparison. No native replacement or
whole emoji runtime completion is claimed here.

`provenance.json` records the unchanged checker, source helper, font and native
example SHA-256 values, Python/Pillow/NumPy versions and installed codec/shaper
versions. `original-cases.json` summarizes 21 complete original CLI executions with
controlled peers. The peer reads the source PNG written by the original checker
before each candidate invocation; expected pixels are never obtained from the Rust
emoji implementation. Full logs, commands and images remain in ignored scratch
`.analysis/scratch/2026-10-15-ui-emoji/`.

The unchanged original accepts reencoded PNG and lossless TGA, TIFF and WebP even
when the candidate filename ends in `.png`. Equivalent high-byte RGBA16 PNG also
passes. Comparison uses decoded array values, including RGB under zero alpha, not
encoded file bytes. A one-pixel difference or hidden RGB difference fails only after
all seven invocations and the final metrics receipt. A later-case difference also
preserves the complete receipt.

Pillow does not convert the candidate to RGBA: grayscale, palette indices and
16-bit grayscale retain their array modes and NumPy broadcasting rules. The
captured 1x1 grayscale, 4x128 grayscale/palette and 16-bit grayscale cases reach all
seven invocations and produce metrics before failing. Full-size RGB and grayscale
instead fail shape broadcasting on the first candidate without a metrics receipt.
Missing, malformed, truncated and nonzero-exit candidates preserve distinct partial
artifacts. Child stdout/stderr and stdin are inherited; the original has no timeout.

A future native checker must preserve these cases or record an explicitly approved
contract change. Existing locked `png` 0.17.16 alone cannot provide broad-format
admission. No codec dependency is added by this evidence unit. The generic source
emoji parser/raster runtime is a separate pending dependency; freezing seven test
inputs would not replace it. Source/font/reference-library drift must fail closed
and require independently recaptured reference evidence.
