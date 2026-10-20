# Bluetooth enumeration live reference

Run the retained `rust/tools/check_bluetooth_enumerate.py` with Python 3.12 and
the native `check-bluetooth-enumerate` in both present-checkout and
historical-checker-absent copied-root lanes against the **same freshly built**
`openpilot-bluetooth` `input_fixture` ELF. Give each controller a distinct fresh
output directory and capture its raw process stdout and stderr separately. The
reference test requires all nine paths through
`BLUETOOTH_ENUMERATE_{ORIGINAL,NATIVE,ABSENT}_{OUTPUT,STDOUT,STDERR}` and the
`BLUETOOTH_ENUMERATE_ABSENT_ROOT` path.

The corpus has the eleven original named nodes and the 29 code points that
CPython 3.12 treats as whitespace: U+0009..U+000D, U+001C..U+0020, U+0085,
U+00A0, U+1680, U+2000..U+200A, U+2028..U+2029, U+202F, U+205F, U+3000.
The independent Rust oracle pins this set explicitly and never asks the
candidate's enumeration, strip, or address helpers for an expected value.

Keep the original Python checker and its raw `result.json`, stdout, and stderr
as evidence. Both native lanes also keep both fixture invocations' raw
stdout/stderr. The reference compares the two native results against the live
original, normalizing only their output-root prefixes. The absent lane runs
under an exact executable trace requiring one controller and two fixture ELF
invocations, with no Python or other executable. A successful receipt records
source and ELF SHA-256 and is published only after the held source, binary, and
node files are rechecked. Cloud acceptance of the new lane is pending.
