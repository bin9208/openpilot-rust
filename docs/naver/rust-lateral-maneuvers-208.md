# Rust lateral maneuversd

Issue: [#208](https://github.com/bin9208/openpilot-rust/issues/208).
Source: `openpilot/tools/lateral_maneuvers/lateral_maneuversd.py`, original MIT license.

`openpilot-lateral-maneuversd` preserves the six source presets, captured baseline
curvature, readiness, steering/speed reset, repetitions, completion delay and
setup-alert holdoff. It waits for CarParams, polls modelV2 with the source
one-second timeout and publishes alertDebug then lateralManeuverPlan. Source
plan validity depends on active maneuver/completion state, independently of
input all_checks. The last completion-holdoff update can publish valid baseline
curvature before selecting the next preset; this source behavior is retained.

Independent execution requires CarParams and the five original input topics:
carState, carControl, controlsState, selfdriveState and modelV2. Native dependencies
are Linux clocks/signals, cereal/msgq, Params and libzmq logging. Python is an
oracle dependency only. CLI and imported main have the same source behavior;
production daemon selection remains unchanged.

## Focused evidence, 2026-10-07

- Original main and native trace: all six presets completed over 2,620 steps,
  zero state, validity or decoded-payload differences. Raw sine acceleration
  maximum error was zero; the checker allows only 1e-15 absolute raw sine error,
  while all wire Float32 fields, baseline and state remain exact.
- Actual private Params/msgq: 2,620 matching publications per topic. Covers
  initial model timeout, invalid inputs with an active plan, readiness/reset,
  six final-holdoff baseline publications and the finished stream.
- Params IO directory-to-value recovery, waiting SIGTERM exit 0, malformed
  CarParams exit 1, invalid frame count exit 1, three direct tests and selected
  clippy passed. Existing shared maneuver policy was reused unchanged.
- Parent reviewed the source main against owner, wire and runtime: polling,
  validity, baseline capture, completion/reset and alert holdoff match.

Independent evidence is preserved in the earlier control-tools worktree under
`.omo/evidence/control-tools-208/lateral-20261007/`; IPC receipt is
`compare-ipc/receipt.json`. Daemon SHA-256:
`d4f26258d66bdd3a7d85e1f6d5fc61cb55e3bf17ea390e05557cb03a357c7dbc`.
The full corpus is reused for integration; only a short local startup/IPC subset
is rerun. Existing focused control-tools Actions includes the source comparison.

The clean integration candidate passed three direct tests and a 20-step real IPC
startup subset, with 20 matching publications per topic, initial model timeout
and matching waiting/malformed exits. This subset selects one preset and does
not reach active-invalidity or final-holdoff cases; those reuse the full run
above. Receipt: `.analysis/scratch/2026-10-07-lateral/dev-ipc/receipt.json`.
Integration daemon SHA-256:
`a3750afa2e940921014d5bb4080a7c41075da59afed3000b349e63124bb1868f`.

AGNOS/device execution and complete normal startup/upload remain #1 acceptance
gates. No vehicle, physical controller or CPU benefit is claimed. The absent
optional webjoystick source remains an explicit separate inventory boundary.
