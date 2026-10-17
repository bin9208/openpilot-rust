# Bluetooth digit generator reference

`original.rs` is the exact stdout of the unchanged
`rust/tools/generate_bluetooth_digits.py` on CPython 3.12.15 / Unicode 15.0.0.
The original exhaustively scans all 1,114,112 code points using `isdigit` and
`unicodedata.decimal`, verifies every decimal sequence, and prints both tables.

- Original script SHA256: `1bd9376e53579ab9d45ec0f0301ef61734986182ee33d7cad6947c04d95f80d4`.
- Original output SHA256: `f3f6350521974c9213f55fded4ac23b8fe3ae1691f7a8869d6945e0267186d32`.
- Capture: `/home/fma1/rust/openpilot-rust/.venv/bin/python -P -B rust/tools/generate_bluetooth_digits.py`.
- The legacy generated comment is retained as output provenance, not an executable dependency.

Native generation uses the independent, unmodified Unicode Character Database
15.0.0 `DerivedNumericType.txt` in `../../../data/unicode-15.0.0/`, rather than
the original output or runtime tables. Its source URL is
https://www.unicode.org/Public/15.0.0/ucd/extracted/DerivedNumericType.txt and
SHA256 is `710abf2d581ac9c57f244c0834f9d9969d9781e0396adccd330eaae658ac7d6b`.
The Unicode license is included beside the data. Decimal and Digit properties
form Python's digit predicate; only Decimal contributes to the decimal starts.
