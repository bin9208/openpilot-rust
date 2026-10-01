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
- Command journal comparison:721 operations match file values and rewrite
  decisions after normalizing the random UUID session. Both channels retain
  the64-event bound,0.4-second expiry, hold replacement and repeated flags.
  Files remain mode0600 and temporary files are removed.
- Atomic JSON comparison:15 source/native file scenarios match exact bytes
  and permissions, including all1,112,064 Unicode scalar values and three
  encoding failures that preserve an existing file. Two ownership tests cover
  serialization failure and replacement of a formerly world-readable file.
- Daemon state processing:26 scenarios and337 iterations execute the unchanged
  Python main loop with owned pipes, scripted vehicle snapshots and clocks.
  Native journals, status/history, reload/open/close decisions match after UUID
  identity normalization. Cases include11 held-command interruption paths,
  learning expiry/nonfinite values, config changes, reconnection/errors,
  throttle/history limits and stale/future event timestamps. The source fixture
  substitutes only IPC, clock and input discovery/ownership boundaries; this
  does not yet verify actual native evdev or cereal transport.
- Strict package Clippy, Rust formatting, Python Ruff and whitespace checks pass.

Native Linux input ownership now opens nonblocking read-only descriptors, grabs
the device, selects its monotonic event clock and drains buffered fragments.
The two evdev ioctls are confined to a small audited boundary; event parsing
uses safe native-endian byte conversions. Descriptor and permission-child
ownership are released on failures. The existing three-second sudo timeout,
command arguments and permission-fallback eligibility are preserved.

- Sixteen original/native FIFO scenarios match outcomes, error messages,
  ioctl order, permission command arguments and zero descriptor growth. Cases
  include partial/empty input, interrupted reads, ownership failures, permission
  failure, missing sudo, terminated children, timeout and full output pipes.
- 10,049 decode cases match signed fields and binary64 timestamp bits exactly.
- Forty synthetic sysfs nodes produce the same 31 accepted devices in source
  order, including all Python whitespace characters; malformed UTF-8, missing
  files, non-Bluetooth nodes and invalid addresses are excluded.
- Rust AddressSanitizer passes all sixteen input scenarios and decode cases.
  The separate C syscall fixture passes UndefinedBehaviorSanitizer. Miri with
  strict provenance, symbolic alignment and preemption checks passes the safe
  decoder regression. Miri does not execute kernel ioctls; these checks do not
  establish physical HID behavior.

Input evidence is retained in `input-io-expanded-green`, `input-decode-green`,
`enumerate-green`, `input-asan`, `input-decode-asan` and `input-ubsan`.
The final Miri run uses `input-miri-target` after the reused cache reported
missing dependency metadata; `input-miri-isolated.log` records the clean pass.
No system input permissions or Bluetooth services were changed by the fixtures.

Source SHA-256:
`74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633`.
Exact binary hashes, inputs, original/native outputs and command logs are in the
private `2026-10-01-rust-bluetooth` scratch directory, under
`gesture-policy-final` and `config-policy-final`.
Journal and file results are in `journal-utf8` and `files-first`. Shared JSON
serialization now has an explicit UTF-8 file method; the existing ASCII-escaped
log formatter remains unchanged and its package tests pass. The new method
retains Python nonfinite values and rejects lone surrogates at the UTF-8 file
boundary.
Daemon-state results are in `engine-clock-corrected`. Its first harness attempt
advanced time inside `SubMaster.update`, after the source's loop timestamp was
sampled; that produced a false reload-cadence mismatch. The corrected fixture
provides each scripted timestamp before the loop begins, without changing the
original daemon. The native long-hold disengagement regression also passes.
The command-file reader compatibility repair is tracked separately in
[#159](rust-command-json-159.md).

Run `cargo build --manifest-path rust/Cargo.toml -p openpilot-bluetooth --examples
--locked -j2` with incremental compilation disabled after the required disk
preflight. Then run `rust/tools/check_bluetooth_gestures.py` and
`rust/tools/check_bluetooth_config.py`, each with `--binary` pointing to its
matching `gesture_fixture`/`config_fixture` executable and `--output` pointing
to an owned evidence directory.

## Still in progress

Native daemon lifecycle,
private BlueZ protocol comparisons and actual
native IPC remain to be implemented/verified. This stage does not mark the
component ported and does not establish a complete runtime candidate, physical
Bluetooth behavior, vehicle acceptance or CPU savings. No device or system
Bluetooth service was accessed.

Docs-Not-Needed: implementation-language conversion of existing behavior; no
setting or user-visible behavior change.
