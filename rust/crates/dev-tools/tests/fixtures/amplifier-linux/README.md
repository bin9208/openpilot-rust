# Independent Linux amplifier source evidence

`original27.json` records actual Python 3.12.15 processes using unchanged
`amplifier_linux_source.py`, `common/i2c.py` and the selected-file preload
fixture. It captures ioctl order/errno, struct/union sizes and alignment,
explicit descriptor closure, register bytes and observable results for all
27 original checker cases. Its four source hashes remain live drift guards.

`generic-original.json` adds 17 actual original helper executions: empty
transactions, negative/wrapped/huge integers, boolean bytes, unknown action
kinds, missing fields, invalid numeric values and invalid action containers.
Original action text is retained so a JSON fixture reader cannot round huge
integers before the reference runs. `cli-original.json` captures six unchanged
checker help/argument cases.

`syscalls.c` is a minimal Linux UAPI provider, not captured policy. Rust owns
arbitrary action iteration, forced-address ordering, conversions, errno
propagation and close precedence. A strict subprocess protocol exposes only
open, forced-address ioctl, SMBus-byte ioctl and explicit close. The Rust owner
has no preload library; only this syscall child loads the fixture. This avoids
process-global environment mutation and a second fixture destructor overwriting
register state. The candidate's LinuxPlatform/I2C library is not reused.

The native checker requires a C compiler to build this small host boundary.
The two original Python tools are retired after independent review. Their explicit
retirement identities below preserve provenance after deletion.
These are Linux fixture results, not actual I2C hardware/device validation.

Reviewed retirement identities now allow the two original Python tool files to
be absent only while the complete `original27.json` bytes and their recorded
source identities match explicit SHA256 constants. Changed reintroduced tools
are rejected. The unchanged `common/i2c.py` and preload fixture remain required.

The native source invocation receipt intentionally differs from Python: it
records the actual Rust owner's empty `LD_PRELOAD`, its `AMP_SYSCALL_HELPER`, and
`AMP_SYSCALL_PRELOAD` used only for the raw child. It never reports the child's
preload as though it had been applied to the owner. Older artifact proofs that
normalized only argv do not cover this corrected receipt. Fresh review proof
compares all 483 artifacts and separately verifies all 27 actual source receipts.

`splitlines-original.json` captures 13 original trace admission cases, including
Unicode separators and repeated blank lines. `early-original.json` captures six
input/path failures before open: the original preload destructor still writes
initial register bytes. The Rust raw owner starts before parsing, receives no
open command on these failures, and exits gracefully to preserve that behavior.

The missing-input CLI capture also preserves initial state without any ioctl.
An empty --case value selects all 27 cases, matching the original falsy filter.
