# Native Bluetooth input conversion (#155)

Tracks [#155](https://github.com/bin9208/openpilot-rust/issues/155) under the
full-runtime conversion in #1. The preimplementation scope and numeric policy
are in [the contract](../rust-port/bluetooth-contract.md).

The first implementation stage provides source-compatible configuration
normalization and the HID gesture state machine. It retains Python dictionary
order, full Unicode name conversion/truncation, normalized-address overwrite
order, exact validation errors, complete SYN frames, dropped-frame recovery,
short/double/long gestures, direction changes, cancellation, repeat cadence and
maximum hold age. Linux signed32-bit coordinate differences widen before
subtraction. Gesture time comparisons retain the original binary64 arithmetic
and thresholds. The existing native command reader remains the consumption
boundary.

## Verified host evidence

The unchanged original `bluetooth/model.py` is executed independently from the
Rust examples. Comparisons use the existing Yiser seven-button capture,
explicit next-representable timestamp boundaries, adversarial signed input
coordinates and deterministic randomized transitions.

- Gesture comparison:37 scenarios,260,012 operations and1,024 emitted tokens.
  Ordered tokens, active long gestures and repeated-token sets all match.
- Configuration comparison:253 cases,97 accepted. Normalized values and
  device/mapping order match; invalid cases retain source error messages.
  Includes strict booleans, count limits, malformed tokens, Unicode uppercase
  expansion, lone surrogates, nonfinite names and arbitrary integer names.
- Strict package Clippy, Rust formatting, Python Ruff and whitespace checks pass.

Source SHA-256:
`74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633`.
Exact binary hashes, inputs, original/native outputs and command logs are in the
private `2026-10-01-rust-bluetooth` scratch directory, under
`gesture-policy-final` and `config-policy-final`.

Run `cargo build --manifest-path rust/Cargo.toml -p openpilot-bluetooth --examples
--locked -j2` with incremental compilation disabled after the required disk
preflight. Then run `rust/tools/check_bluetooth_gestures.py` and
`rust/tools/check_bluetooth_config.py`, each with `--binary` pointing to its
matching `gesture_fixture`/`config_fixture` executable and `--output` pointing
to an owned evidence directory.

## Still in progress

Command journals and atomic files, native evdev ownership and daemon lifecycle,
source driving/learning gates, private BlueZ protocol comparisons and actual
native IPC remain to be implemented/verified. This stage does not mark the
component ported and does not establish a complete runtime candidate, physical
Bluetooth behavior, vehicle acceptance or CPU savings. No device or system
Bluetooth service was accessed.

Docs-Not-Needed: implementation-language conversion of existing behavior; no
setting or user-visible behavior change.
