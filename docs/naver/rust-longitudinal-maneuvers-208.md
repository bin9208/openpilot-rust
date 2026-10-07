# Rust longitudinal maneuversd

Issue: [#208](https://github.com/bin9208/openpilot-rust/issues/208).
Source: `openpilot/tools/longitudinal_maneuvers/maneuversd.py`, original MIT license.

`openpilot-longitudinal-maneuversd` preserves all seven source presets and their
readiness, action progression, repeat/reset and completion selection. It waits
for CarParams, polls modelV2 with the source one-second timeout, and publishes
alertDebug, longitudinalPlan and driverAssistance in source order. Timeout or
invalid input still advances the source state; plan validity uses all_checks.
An already-active maneuver continues after longActive falls, as in the source.
The CLI and imported main share the same behavior in the original.

Independent execution needs CarParams and five declared inputs: carState,
carControl, controlsState, selfdriveState and modelV2. No other ported daemon is
required. Linux clocks/signals, native cereal/msgq, Params and libzmq logging are
the runtime dependencies. Python is an oracle dependency only. The manager
catalog marks an isolated candidate; production selection remains unchanged.

## Focused evidence, 2026-10-07

- Original main comparison: 4,214 steps, all seven presets selected/completed,
  zero differences in Float64 acceleration, per-step state/selection and complete
  decoded payloads on all three output topics.
- Real IPC: 4,214 publications per output topic, matching the original trace.
  Includes initial model absence/timeout, invalid plan and recovery, repeat
  readiness holdoff, active longActive falling and the finished stream.
- Params read-error recovery, waiting SIGTERM exit 0, malformed CarParams exit 1,
  invalid frame-count exit 1, three focused regressions and selected clippy pass.
- Existing sequence/readiness policy was reused unchanged. Main-session review
  checked source polling, validity, selection, stopping threshold and publication
  fields; no new unsafe code or external numerical library was introduced.

Full independent evidence is preserved in the earlier control-tools worktree:
`.omo/evidence/control-tools-208/longitudinal-20261007/compare-ipc-v3/receipt.json`.
Its daemon SHA-256 is
`86928cd5dbbfc3a575d8f6d98a1b6ade5748d3c2c49355f42b2e52cdf3ddf43a`.
The unchanged full corpus is reused for integration; only a short startup/IPC
composition is rerun locally. The focused control-tools Actions job reproduces
the full independent evidence, with existing required workspace/ARM gates.

The clean integration candidate passed the three direct regressions and a
20-step real IPC startup/invalidity/recovery subset without repeating the full
corpus. Each output topic published 20 matching messages; waiting shutdown and
malformed CarParams exits matched. Receipt:
`.analysis/scratch/2026-10-07-longitudinal/dev-ipc/receipt.json`.
This shorter run covers one preset; all-seven completion evidence remains the
independent 4,214-step run above. Integration daemon SHA-256:
`1ab6f2cf5649be7f41cc9976c6e0fc9f57ae039f34361b4d7976a8b57fc4b896`.

Lateral maneuversd remains unfinished; the shared lateral policy is intermediate
code, not a completed daemon. AGNOS/device behavior and complete startup/upload
remain #1 acceptance gates, and no vehicle or CPU benefit is claimed.
