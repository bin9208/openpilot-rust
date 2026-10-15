# Original hardware command assertion

The unchanged `.github/workflows/rust.yml` inline Python checker was executed
with Python 3.12 against independently generated temporary output files.
Original source SHA256:
`56c2016ae30ecbc8bbe5812c96dc3355912172a5a2d1bf27bd3d026403c7bb3e`.
`original.json` retains the workflow blob and 30 original byte inputs and
admitted/rejected results; the source/capture recipe remain in private scratch
`.analysis/scratch/2026-10-15-hardware-command-check/`.

JSON object order is irrelevant, duplicate keys keep their last value, and
Python numeric equality is retained: integer-valued floats equal integers and
`true` equals numeric one. String/collection truthiness is not equality.
Missing/extra fields, wrong values, malformed JSON, nonfinite values and lone
surrogates are rejected. The inherited stderr file must decode as UTF-8 and
contain exactly `inherited-stderr`, with no trailing newline or NUL.

The existing `hardware_control_command` example is retained. It invokes only
fixed owned shell commands, reads 131072 zero bytes, and probes a deliberately
missing executable through `openpilot-process-child`. It does not instantiate
hardware platform/I2C control. Real source-matched fixture execution and the
original/native checker comparison passed in addition to these cases. The
fresh fixture executable SHA-256 is
`a3fd7bb26e8703f55f7426ac08babe9f6d6713ea2d37bd7e46cab82930f1ca26`;
its Rust source SHA-256 is
`477e199e22c401b766fb55a37b8cf2616b1d639b5076d5773c7ccb21bd7593af`.
The actual comparison result and native checker identity are recorded in
private scratch `actual/result.json` under the capture directory above.
