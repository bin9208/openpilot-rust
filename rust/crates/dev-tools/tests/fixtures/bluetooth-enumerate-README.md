# Bluetooth enumeration live reference

Run the retained `rust/tools/check_bluetooth_enumerate.py` with Python 3.12 and
the native `check-bluetooth-enumerate` against the **same freshly built**
`openpilot-bluetooth` `input_fixture` ELF. Give each controller a distinct fresh
output directory and capture its raw process stdout and stderr separately. The
reference test requires all six paths through `BLUETOOTH_ENUMERATE_{ORIGINAL,NATIVE}_{OUTPUT,STDOUT,STDERR}`.

The corpus has the eleven original named nodes and the 29 code points that
CPython 3.12 treats as whitespace: U+0009..U+000D, U+001C..U+0020, U+0085,
U+00A0, U+1680, U+2000..U+200A, U+2028..U+2029, U+202F, U+205F, U+3000.
The independent Rust oracle pins this set explicitly and never asks the
candidate's enumeration, strip, or address helpers for an expected value.

Keep the original Python checker and its raw `result.json`, stdout, and stderr
as evidence. The native controller also keeps both fixture invocations' raw
stdout/stderr. A successful receipt records source and ELF SHA-256 and is
published only after the held source, binary, and node files are rechecked.
