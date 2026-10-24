# Original control-tools policy comparator

`original.json` captures the unchanged `rust/tools/check_control_tools_policy.py`
against controlled subprocess results and real temporary input/output files. The
fixture records the source SHA-256, interpreter, raw input/expected/output bytes,
process status, comparison outcome, exact partial artifacts and printed receipt.
The expected results were produced by the original implementation, not Rust.

The 55 cases include successful state comparisons, Python numeric equality and
large-integer rounding boundaries, NaN container identity, duplicate keys,
Unicode/lone surrogates, UTF-8/16/32 byte decoding, mismatch truncation at 30,
shape and decode failures, process failures, existing output and raw stdin.
These comparator cases do not replace the original control-state producer or
actual joystick/IPC lifecycle evidence.

Actual unchanged source producers generated 11 cases / 4,216 steps for joystick
input and control. Both original and native comparators emitted byte-identical
stdout, stderr and receipt files for both native probes. Deliberately corrupted
expectations were rejected with identical mismatch receipts in both families.

Independent review added raw-vs-escaped surrogate pairs and distinct keys that
serialize identically; decoded values remain distinct through comparison and
receipt emission. Pinned CPython 3.12.15 depth captures preserve accepted nested
equality at payload depth 9,995 and pre-spawn decoding rejection at 9,996, plus
separate indent=2 serialization failure boundaries for scalars and empty
containers. A 300-second owned process deadline is an explicit execution-policy
addition; original comparison had no subprocess timeout. Native pipe I/O keeps
raw bytes and concurrent input/output behavior. Original producers and binding
compilers remain pending; this removes only their comparator driver.
