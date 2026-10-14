# QCOM diagnostic range admission (#320)

Scope: reject malformed log-configuration range responses before allocating or sending any mask; preserve valid firmware command bytes/order. The source change and host evidence below are separate from target sealing and vehicle acceptance.

`setup::logs` reads sixteen u32 counts. Its input identifiers are u16, with the equipment in the high four bits and the item in the low twelve bits (`typ & 0xfff`). Each equipment can therefore represent 4096 items, indices 0..4095; accepted counts are 0..4096 inclusive. The existing comment stated the twelve-bit contract but no bound enforced it. A malformed late count could allocate an excessive mask after earlier mask commands had already configured the modem. Original `openpilot/system/qcomgpsd/modemdiag.py::setup_logs` also trusts counts; this is malformed-input hardening, not a demonstrated Rust-only regression.

Five private-PTY tests execute real `Diagnostic::exchange` and `setup::logs`: malformed late count prevents all mask commands; every equipment position rejects 4097 and u32::MAX; every truncated range response, wrong operation/status/opcode rejects before masks; valid boundary counts 1/9/4095/4096 preserve exact original-algorithm bytes/order; all-zero ranges issue only the query. The independent expected-mask loop reproduces the original source's equipment/item membership algorithm rather than using the native optimized mask builder. It additionally matches all bytes against `tests/data/log_masks_original.json`, captured by executing the unchanged original `setup_logs` AST with those boundary ranges/types. The fixture retains the source SHA256 `0f064c18d5cdc6fb331f7447182a8ee580d917f65f5d43ec9bd649f487dddf1b` and its inputs.

Initial response-opcode validation is a separate admission check in this patch. The original initial range query also ignores its returned opcode; only the subsequent original mask response asserts opcode 115. Both observed issues are inherited behavior, not demonstrated Rust-only regressions.

## Host verification (2026-10-14)

Canonical environment, shared Cargo target, two jobs, incremental compilation disabled; each build passed the 25 GiB plus 1 GiB growth guard (over 374 GiB free).

- RED: bounded late count4097 was accepted and sent two mask commands. No u32::MAX case was executed until after the fix. Separate malformed-header test reached the wrong-opcode case and failed because it was accepted.
- GREEN: `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd` passes all ten tests: three existing framing, five new real-PTY mask admission, two existing NMEA. The new tests cover all sixteen invalid positions, counts4097/u32::MAX, all75 truncated lengths, wrong operation/status/opcode, zero counts and exact valid boundary bytes/order.
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd --all-targets -- -D warnings`: pass.
- `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd --bins --examples`: pass.
- `check_qcomgps_reference.py`: 204 unchanged-original/native publication comparisons pass.
- `check_qcomgps_daemon.py`: all six private-PTY/loopback scenarios pass (late assistance/signal cleanup, injection failure, CRC failure, length failure, disconnect and AT retry). Original setup/teardown command transcript comparison passes, including unchanged valid masks. First direct shell launch ended with SIGHUP at PTY cleanup after its first scenario had passed; rerunning the unchanged checker as an ordinary subprocess under a Python parent avoids its exclusive-lock probe acquiring a controlling PTY as a session leader. No signal, assertion or production behavior was weakened.

Local logs, original-fixture capture script and comparison evidence are ignored under `.analysis/scratch/2026-10-14-qcom-bounds-320/`.

## Delimiter-free framing remains open

Inspected `framing::{Frames,decode,encode}`, `serial::{recv,exchange}` and original `ModemDiag.recv`. Native `Frames` accumulates until 0x7e, as the original accumulates serial chunks until the same delimiter. The 0x10000 serial read buffer is a transfer chunk, not evidence of a maximum HDLC frame. The general envelope has no length field; the DIAG_LOG payload's length applies to that opcode only, while exchanges also admit non-log responses. No documented maximum for all admitted diagnostic opcodes was established from these sources. No arbitrary truncation cap is introduced. A protocol/firmware maximum and a specified oversize recovery policy are still needed to close this separate resource-bound concern.

No physical modem/device connection, CPU benefit, target artifact or vehicle validation is claimed.
