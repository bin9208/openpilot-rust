# Independent plot sampling evidence

The original `check_ui_plot_samples.py` generated 5,000 seeded steps with53
full-history boundaries. Its `plot_samples_source.render` extracted the original
`DebugPlot` sampling gate and classes from the pinned source. The original
implementation, not the Rust candidate, produced every expected snapshot.

`original.json.gz` also contains108 generic helper cases, including all680
CPython3.12 decimal digit codepoints, whitespace/float admission, lazy errors,
resets, backwards/nonfinite clocks, ring wrap and monotonic queue expiration.
`manifest.json` pins original sources and compressed/decompressed captures.

`checker-original.json` records13 independent executions of the original checker
with a subprocess-only peer: successful outputs and stderr, nonzero child exit,
malformed JSON, short/long/mismatched output and each artifact write failure.
Only temporary output paths in exception text are normalized to `@OUTPUT@`.
Expected file bytes are recorded by SHA256 and length; the fixed source/input
bytes are retained in the compressed corpus. This is process-contract evidence,
not a real candidate test or completed Python retirement.

`checker-responses.json.gz` preserves original subprocess-peer stdout/stderr
bytes, deduplicated by identical value. Replay uses those bytes directly;
re-serializing through a sorted object map would alter key order and hide byte
contract differences. `repr-nonprintable.json` independently captures every
CPython3.12 `str.isprintable()` interval for float conversion error messages.
