# Bluetooth command-reader JSON compatibility (#159)

The original reader accepts Python JSON extensions, including nonfinite numbers
and escaped lone Unicode surrogates. The initial native reader used `serde_json`,
which rejects these documents and treated the entire file as absent.

An actual-file source/native comparison reproduced four differences in eight
cases: `Infinity` in a learning expiry or cancellation cutoff, an unrelated lone
surrogate in a valid learning document, and unrelated `NaN` in a cancellation
document. The source blocked the pending fresh command; native returned it.
Finite controls agreed and both implementations consumed the same unique ID.
This is a synthetic compatibility finding, not an observed vehicle incident.

The reader now uses the existing native Python-compatible JSON decoder from
`openpilot-logmessaged`. Unicode IDs retain their codepoints for once-only
consumption, including escaped lone surrogates. Time gates, cancellation, learning,
poll cadence, bounded history and repeat truthiness retain their source rules.
No Python interpreter is called. This adds a Rust crate dependency on the existing
logging implementation and its declared native dependency graph; extraction of
the shared decoder is not part of this fix.

The original eight cases now agree. The expanded 23-case comparison also covers
nonfinite and overflowing times, distinct/duplicate surrogate IDs, nonfinite
repeat values, malformed documents and valid messages following invalid entries.
The existing 1,026-read corpus returns 381 commands with exact action, ID and
repeat state. Run both `rust/tools/check_command_json_boundary.py` and
`rust/tools/check_command_reference.py` with the built `command_probe`; the
workspace CI runs both. Local red/green reports are preserved under
`.analysis/scratch/2026-10-01-rust-bluetooth/reader-boundary-*`.

This covers the command-file boundary within #155, not the complete Bluetooth
daemon or full startup/upload candidate. #1 and device acceptance remain open.
