# Rust joystickd continuation

Issue: [#208](https://github.com/bin9208/openpilot-rust/issues/208).
Restart basis: the owner's 2026-10-07 Rust port restart instructions.

This increment ports only `openpilot.tools.joystick.joystickd`: the vehicle-model
calculation, joystick expiry, activation and cruise flags, and the continuous
100 Hz `carControl` / `controlsState` publisher. It reuses the existing Rust
vehicle model, Params, cereal, messaging, logging and rate keeper. The joystick
input adapter and both maneuver daemons remain separate unfinished #208 work.

The manager catalog advertises an isolated candidate; production selection is
unchanged. A missing optional joystick process can still be excluded through
the existing manager predicate. Complete runtime startup, log upload, AGNOS
execution and user device comparison remain #1 acceptance gates.

## Independent execution

Build `openpilot-control-tools` with its default `native` feature and run
`openpilot-joystickd`. Provide serialized `CarParams` in the existing Params
namespace, and the original `carState`, `onroadEvents`, `liveParameters`,
`selfdriveState`, and `testJoystick` topics. No other ported daemon is required;
the test launches original C++ msgq peers as the input/output boundary.
`--frames N` bounds host execution; SIGINT/SIGTERM stop startup waiting and the
running loop. Runtime dependencies are Linux clocks/signals, cereal/msgq,
Params data and libzmq logging. This dev-based increment still uses the existing
C++ msgq adapter; the separately implemented Rust transport remains an explicit
unmerged integration candidate.

## Focused evidence, 2026-10-07

- Source comparison: 8 cases, 1,805 steps, zero differences in Float32 actuator
  and curvature bits, activation/cruise flags, and IndexError/ZeroDivisionError
  publication order. Includes clipping, zero/stopping/high speed, roll/angle
  offset, 20/21-frame expiry, missing axes, and longitudinal/PCM configuration.
- Package test: four joystickd regressions pass. Worker package clippy with
  warnings denied and formatting pass; clean dev composition builds and the
  same four focused tests pass. Inherited C++ compiler warnings remain.
- Actual IPC: five source-matched phases (fresh, expired, recovered, override,
  disabled), directory/read-error recovery before CarParams appears, malformed
  CarParams failure, waiting SIGTERM and active SIGINT exit 0, and three invalid
  CLI cases exit 1. The process runs 240 iterations; the original subscriber
  captures 239 pairs after publisher takeover, over 2.380007155 seconds
  (99.9997 Hz). Subscriber warmup is not a runtime missing-output claim.
- Main-session review: each file has a single boundary/policy/runtime role,
  typed inputs and exhaustive wire variants; no new unsafe code or dependency
  version changes. Original algorithm and source publication order are retained.

Source evidence is preserved in the earlier control-tools worktree at
`.omo/evidence/control-tools-208/joystickd-20261007/{source,compare,ipc-v4}`.
The clean dev connection receipt and packets are at
`.analysis/scratch/2026-10-07-joystickd/dev-ipc/`; its binary SHA-256 is
`8b0470bc428207b25e9173b58bda4cc77ec19b21427eb412275edda2b2ace356`.
The source corpus is reused because the policy, vehicle model, cereal schema,
Python dependencies and inputs did not change. Only the changed dev IPC boundary
was rerun; unrelated candidates were not rebuilt or rechecked locally.

The required Actions job reproduces source and actual IPC checks. The existing
required aarch64 workspace build covers the new package. ARM results, remote
mandatory gates and dev merge remain pending until recorded for the PR SHA.
No AGNOS, device, vehicle or performance acceptance is claimed.
