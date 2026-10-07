# Rust joystick input publisher

Issue: [#208](https://github.com/bin9208/openpilot-rust/issues/208).
Source: `openpilot/tools/joystick/joystick_control.py`, original MIT license.
This is the next independent module after joystickd in PR #212.

`openpilot-joystick` implements the manager's imported `main()` path: native
gamepad input and 100 Hz `testJoystick` publication while onroad.
`openpilot-joystick-control --keyboard` implements the guarded manual CLI:
`IsOffroad` must be true unless `ZMQ` is present. The original keyboard and
PC/TICI gamepad mappings, calibration, clipping, deadzone/expo, EOF retention,
I/O-error reset, and JoystickDebugMode write behavior are preserved. Buttons
remain empty as in the original sender. The product CLI does not gain arbitrary
gamepad path selection; the separate example accepts owned test input paths.

Runtime requires Params, native cereal/msgq, Linux clocks/signals, and either a
POSIX terminal or Linux evdev gamepad. Python/inputs are comparison dependencies
only. No other ported daemon is required for independent execution. Production
selection is unchanged; the manager catalog exposes an isolated candidate.

## Focused evidence, 2026-10-07

- Keyboard/PC/TICI source policy: three cases, 2,411 steps, zero differences in
  Float64 axes, calibration and cancel state. Three direct input regressions
  and selected package clippy with warnings denied pass.
- Real original-source/native IPC: keyboard PTY nine phases, gamepad FIFO eleven
  phases, actual input I/O error two phases, and managed onroad entry two phases.
  Checks include clipping/reset/Unicode, partial records, absence/recovery,
  EOF retaining axes, input errors resetting axes, ignored Params write errors,
  empty buttons, offroad refusal, ZMQ-presence bypass and non-TTY failure.
- Source and native sender rates remain about 100 Hz. Keyboard terminal settings
  are restored on exit. The managed case publishes with IsOffroad=false and no
  ZMQ; CLI and manager entry policies remain distinct.
- Main-session review confirms typed event decoding, fixed 24-byte Linux 64-bit
  evdev records, safe rustix terminal handling and no new unsafe code. Existing
  joystickd source remains unchanged.

Detailed evidence and final binaries are preserved in the earlier control-tools
worktree under `.omo/evidence/control-tools-208/joystick-input-20261007/`.
The receipt index is `README.md`; final native-boundary evidence is `ipc-v4/`.
CLI binary SHA-256:
`18746f66c7d91ec0bf344bf2765f62a590d878663924abce85ebe7099cebf6df`.
Managed binary SHA-256:
`e5b2a31a1b10b70175f5254ad815c3f0288472ee2c0b7c357b25b6f80638dd56`.

Clean dev composition on `ea59d1cc` passes the three focused input tests, the
selected binary/example build, formatting, and the same real PTY/FIFO/IPC
boundary checks. Receipt: `.analysis/scratch/2026-10-07-joystick-input/dev-ipc/`.
Its CLI binary SHA-256 is
`2a8d1b6ef4d7409e111545e43cc703aae54a49ecdd10150c3ba7837be2be6fc4`.
Only the changed dev IPC boundary was rerun; the unchanged 2,411-step policy
corpus and existing joystickd module evidence are reused.

The focused joystick Actions job includes source and actual IPC checks; existing
required workspace/ARM gates remain. Physical controller discovery/unplug,
AGNOS/TICI execution, whole startup/upload and user device acceptance remain
unverified. Longitudinal/lateral maneuvers remain separate unfinished #208 work.
