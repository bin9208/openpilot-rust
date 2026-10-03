# Native Bluetooth runtime contract (#155)

This stage replaces project-owned Bluetooth input, mapping, gesture, command
publication and BlueZ coordination. Source behavior is defined by
`openpilot/selfdrive/carrot/bluetooth/{model,daemon,bluez}.py`; the web feature
adapter belongs to the subsequent native Carrot server integration. Existing
native command consumption in `rust/crates/desire/src/command.rs` is reused.
No physical input device, system BlueZ service or paired vehicle is used for
host verification.

## Required equivalence

Mappings retain source insertion order, normalization, Unicode name truncation,
strict boolean enable flags, 16-device/192-mapping/64-base-token limits and exact
action/token validation. Missing or malformed persisted configuration falls
back to the same empty configuration. Runtime files use exclusive temporary
creation, mode0600 and atomic rename, with source cleanup behavior.

Gesture inputs carry the original Linux event type/code/signed value and
monotonic binary64 timestamp. All token order, short/double/long decisions,
hold cancellation, repeat decisions, SYN_REPORT/SYN_DROPPED behavior, learning
and stale-data decisions must match exactly. Boundaries0.35/0.7/0.5/10/0.4 seconds
are unchanged. Generic tap coordinates use ties-to-even rounding in25-unit
increments. Integer coordinates widen before subtraction to preserve Python's
unbounded arithmetic for every Linux signed32-bit event value.

Timestamp arithmetic uses binary64 in the same operation order. Discrete
decisions and persisted timestamps are exact; independently sampled wall-clock
observations are checked by ordering/deadline bounds, never by widening a
gesture threshold. There is no numeric tolerance for command authorization.
Controlled session identifiers make per-channel event journals comparable:
TTL pruning, replacement of an active hold, sequence order and the64-entry
bound remain source-exact.

The daemon retains polling, reload/status cadence, exclusive grabs, monotonic
input clocks, incomplete-read rejection and disconnect cleanup, car-state
freshness and all brake/gas/gear/button/enabled/learning gates. Source BlueZ
adapter discovery,30-second scan,90-second pairing,60-second prompts,
cancel/reject semantics and object/interface checks are retained through an
owned private D-Bus service. No raw command execution is exposed.

## Evidence gates

Before declaring this component complete, run unchanged source policy over
boundary/adversarial configuration and gesture inputs, including the existing
Yiser capture; compare complete state and ordered outputs with native code.
Then drive the real native process through original cereal IPC, owned input
descriptors, private D-Bus and isolated runtime files. Verify initial state,
reloads, learning, blocked driving states, disconnect/reconnect and signal
shutdown. Exercise native descriptor ownership under sanitizers if a C++
syscall adapter is needed. Retain source/binary hashes and exact commands.

Component results remain intermediate evidence. Full normal startup, log
upload, native dependency inventory and the complete project-owned runtime
are required before the user's first device comparison.
