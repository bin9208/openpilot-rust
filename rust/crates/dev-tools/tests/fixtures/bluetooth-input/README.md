These expectations were captured from the unchanged Python checker and Bluetooth
source, without importing the Rust candidate. `original.json.gz` contains all
25 extrema, 10,000 seeded random records and 24 incomplete buffers, including
binary64 timestamp bits. `manifest.json` identifies the original source hashes,
CPython version and immutable corpus hashes. `boundaries.json` records 14 actual
original checker executions with controlled child output, including Boolean zero,
integer zero, negative zero, cardinality, JSON, child failure and artifact phases.
Tests assert both corpus and boundary hashes before consuming expectations.

The native source contract is the original Linux little-endian LP64
`struct.Struct('@llHHi')`, size 24; unsupported hosts fail explicitly.
`bluetooth_input_check` requires `BLUETOOTH_INPUT_FIXTURE` pointing at the current
`openpilot-bluetooth` `input_fixture` example. Tests execute the real checker,
verify raw paths and arguments, reject source/executable identity changes, and
signal only the checker leader after acknowledging its owned descendant.

Only `check_bluetooth_input_decode.py` is eligible for retirement after the
same frozen ELF passes before/after deletion. The remaining 13 Bluetooth Python
cases and their source helpers are outside this conversion unit.
