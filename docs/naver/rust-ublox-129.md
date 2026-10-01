# u-blox Rust runtime port (#129)

Tracking: [#129](https://github.com/bin9208/openpilot-rust/issues/129), [full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1).
Source baseline: `0893b81c`. Branch: `codex/feat-129-ublox`.

The native receiver/decoder and required UBX/GPS/GLONASS helpers are implemented in `rust/crates/ublox`. Focused actual-source packet/policy comparisons, owned PTY termios and serial I/O, production ioctl sanitizers, safe-parser Miri and both running native daemons provide host evidence. The PTY test exposed a combined-speed-setter flag difference; the source-compatible update passes the complete affected scenario.

The 2026-10-01 filesystem recovery preserved the implementation. A bounded package rebuild, all four focused host gates and final static checks passed again; `.omo/evidence/ublox-129/resume/` retains the new artifacts alongside the original receipt. No runtime code was changed during recovery.

See [scope, commands, outcomes and limits](../rust-port/ublox-validation.md). Parent integration records the final commit, dev PR and exact-SHA Actions URLs in #129. CI integration, generic ARM build and vehicle acceptance are separate states. No production selection, QCOM GNSS, user guides, workflow or physical-device operation is included. The first device comparison remains gated on complete normal startup and the existing log upload path.
