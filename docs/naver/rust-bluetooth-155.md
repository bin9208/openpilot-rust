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

The native `openpilot-bluetoothd` process now connects the input owner and
gesture engine to the existing `carState`, `deviceState` and `selfdriveState`
transport. It retains the source runtime/config paths, nonblocking exclusive
reader lock, polling and reload/status cadence. Explicit path and frame
arguments support owned host validation. It does not launch a Python runtime.

The original main loop and native binary both pass actual cereal IPC plus
owned-FIFO comparisons for offroad suppression, onroad command publication,
invalid carState, invalid CAN, stale carState, brake cancellation of a held
command, learning, partial-read recovery, device disappearance/reappearance
and reader-lock exclusion. All recorded semantic results and five complete
open/grab/clock/close sequences match. These complement the 337 deterministic
engine iterations; sampled runtime clocks are checked by deadlines and state
ordering rather than compared as equal timestamps.

Three additional source/native shutdown scenarios match: SIGINT writes the
stopped status and exits with signal 2; default SIGTERM exits with signal 15
without writing stopped status; SIGINT during a blocked permission command
kills and reaps the owned child before exit. Both ordinary signal paths release
the lock and descriptors so a fresh reader can start. Native exec traces from
bounded process runs contain only the native executable. The original sixteen
input-owner scenarios still pass after adding cooperative interruption.

Actual IPC/lifecycle results are retained in `daemon-ipc-final` and
`daemon-shutdown-final`; individual signal baselines are in
`source-signal-green`/`native-signal-green`. Initial fixture attempts used an
invalid mapping and incorrectly expected a release event after offroad hold
cancellation. Corrected fixtures use source-valid mappings and require
unchanged event state on those blocked inputs; no source behavior was changed.

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

## Native BlueZ coordinator

The Rust library now owns its D-Bus connection, application pairing agent,
scan and pair tasks, cancellation, discovery, device actions and snapshots.
It preserves first-adapter wire order, source defaults, application owner,
interface and target checks, KeyboardDisplay registration, trust/connect
ordering, remote error bodies and paired-with-connect-error state. Pending
calls and spawned work are cancelled and joined when closing the owner.
The existing libdbus/dbus-tokio boundary remains an explicit native dependency.

The unchanged Python source and the native client ran against owned private
`dbus-daemon` instances, with no system bus or physical radio access:

- Policy: 6,363 input cases, 3,598 accepted, match exact responses. All Unicode
  code points were checked against CPython 3.12's Unicode 15 digit/decimal data.
- D-Bus operations: 72 observations, 22 ordered calls and six agent replies
  match, including errors and 20 close/reopen cycles with zero descriptor growth.
- Prompts: PIN, decimal passkey, confirmation, authorization, duplicate prompt,
  Cancel/Release, display overlap and failed serialization match. Thirty
  prompt/conversion scenarios include signed/Unicode/underscore text, finite and
  nonfinite floats, uint32 limits and the 4,300-digit conversion limit.
- Real elapsed-time tests retain 30-second discovery, 60-second prompt and
  90-second pairing deadlines in both implementations. Expired responses are
  rejected and the final prompt/target state matches.
- Invalid NUL-containing PINs expose an inherited source defect tracked in
  [#166](https://github.com/bin9208/openpilot-rust/issues/166). The original
  emits an invalid D-Bus string and loses its connection. Rust sends no invalid
  frame and keeps its connection usable. This invalid-wire case is an explicit
  difference, not counted as exact parity. Lone-surrogate PIN serialization
  fails without a wire reply in both implementations.

Evidence is retained under `bluez-policy-first`, `bluez-reopen-first`,
`bluez-prompts-second`, `bluez-prompts-conversion` and `bluez-invalid-first`.
Each result records the native binary hash; source and native observations are
saved separately. `check_bluetooth_bluez.py`, `check_bluetooth_bluez_prompts.py`
and `check_bluetooth_bluez_invalid.py` are rerunnable private-bus checks.
The combined `check_bluetooth_runtime.py` command passes all fourteen cases
in `complete-runtime-first/suite.json`, including 31 prompt/deadline scenarios
and 62 exact agent replies. The CI connectivity job builds the original msgq
binding and invokes the same command. The small Unicode-file fixture no longer
imposes a workstation-specific 26 GiB free-space requirement on CI; local
build/install/large-copy disk preflights remain required before those operations.
Package tests, strict Clippy, formatting and Python Ruff pass. The inherited
C++ msgq compiler warnings remain separate from Rust diagnostics.

The HTTP adapter is part of the subsequent Carrot server conversion. Normal
manager selection/startup, physical Bluetooth behavior, vehicle acceptance
and CPU comparisons remain outside this component evidence. Full-runtime #1
is open; this is not a first-device handoff.

## CI boundary repairs

The first PR #167 runs exposed a missing candidate entry in the manager catalog;
the manifest and catalog now both identify `openpilot-bluetoothd` as an isolated
candidate without selecting it for production. They also exposed
[#171](https://github.com/bin9208/openpilot-rust/issues/171): GitHub's Python
uses direct `stat64`, which the input fixture did not intercept. The original
therefore saw no synthetic character device and skipped permission fallback.
System Python 3.12 reproduced all seven permission mismatches locally, while
the uv Python used an already covered stat entrypoint. The fixture now covers
direct `stat64` only for its owned synthetic path. All sixteen source/native
cases pass with both Python builds and the fixture compiled under UBSan.
Evidence: `input-system-python-red`, `input-system-python-green` and
`input-uv-python-green`; runtime permission policy is unchanged.

Docs-Not-Needed: implementation-language conversion of existing behavior; no
setting or user-visible behavior change.
## Immediate scan cancellation boundary (#235)

During the #225 HTTP adapter work, an owned private D-Bus comparison found that
immediate `scan` then `close`, before the Python timer task first runs, omits
`StopDiscovery` in the original but emits it in Rust. Python cancellation skips
the unstarted coroutine's `finally`; Rust wakes and joins its watch-driven task.
The existing 30-second timer policy was not changed. Both Rust function bodies
are byte-identical to `edcdbf20e8b4281ac89c46479f93ece9c9552380`; no older ELF
was available, so pre-existing behavior is established by source identity only.

[Issue #235](https://github.com/bin9208/openpilot-rust/issues/235) retains this
unresolved boundary. The failing trace and identity receipt are under
`.omo/evidence/carrot-server-225-resume/bluetooth/`. The separate yielding
request/response comparison passes four commands and ordered calls; it does
not resolve the immediate-call difference. Neither case uses an actual radio.

# Manager catalog follow-up

The candidate manager catalog now records the Bluetooth crate and executable,
matching the runtime inventory. The unchanged-source catalog comparison passed
63 descriptors, 64 configurations, 25 environment snapshots, 10,573 predicate
cases and 24 fatal cases. Evidence is retained in the local
`2026-10-01-bluetooth-ci/bluetooth-catalog-green-full-deps` directory. This adds
candidate availability only; whole-runtime startup remains tracked by #1.
