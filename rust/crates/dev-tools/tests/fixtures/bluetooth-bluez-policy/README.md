# BlueZ prompt policy controller corpus

The native controller preserves the unchanged
`rust/tools/check_bluetooth_bluez_policy.py` schedule: seven prompt kinds,
33 fixed values, every Unicode 15.0.0 Digit or Decimal code point (808),
and every six-character Decimal-zero sequence (68). This is 909 values per
kind and 6,363 responses. The pinned independent property source is
`../../../data/unicode-15.0.0/DerivedNumericType.txt`, SHA-256
`710abf2d581ac9c57f244c0834f9d9969d9781e0396adccd330eaae658ac7d6b`.
The controller does not import production `openpilot_bluetooth` tables or
policy code. It applies the original `Bluez.respond` admission predicates to
its own corpus and compares every native response plus the complete `--digits`
table.

The first cloud run must execute the unchanged original with Python 3.12 /
Unicode 15.0.0 and the same freshly built `bluez_policy` example, then run
the native controller. Upload **visible** original `cases.jsonl`,
`source.json`, `native.jsonl`, `native.stderr`, `result.json`, and original
stdout/stderr, plus a separate raw `bluez_policy --digits` stdout capture from
the same candidate ELF, along with native outputs and source/binary hashes.
The mandatory `bluetooth_bluez_policy_reference` test consumes these paths
through `BLUEZ_POLICY_ORIGINAL_OUTPUT`, `BLUEZ_POLICY_NATIVE_OUTPUT`,
`BLUEZ_POLICY_ORIGINAL_STDOUT`, `BLUEZ_POLICY_ORIGINAL_STDERR`,
`BLUEZ_POLICY_ORIGINAL_DIGITS`, `BLUEZ_POLICY_ORIGINAL_DIGITS_STDERR`,
`BLUEZ_POLICY_NATIVE_STDOUT`, and `BLUEZ_POLICY_NATIVE_STDERR`.
The reference compares the digit probe's captured stderr byte-for-byte;
diagnostic output is not treated as a failure merely because it is nonempty.
The controller translates subprocess text-mode CRLF/CR newlines before saving
the main native output files, matching the original Python checker on Linux.
The separate `--digits` stdout/stderr files retain raw bytes for comparison
with the direct original candidate probe.
Only after
that independent live pair passes should its original output be sealed here;
no native-generated expected file is an original capture. Keep the original
Python checker until a later reviewed retirement gate proves its absence.
