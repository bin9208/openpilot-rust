# Rust modem implementation record

Independent repository issue [#113](https://github.com/bin9208/openpilot-rust/issues/113) implements the project-owned AT/PPP daemon under [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The [technical validation record](../rust-port/modem-validation.md) describes the unchanged-source PTY comparisons, actual continuous native process lifecycle, native external dependencies and remaining Actions/ARM/device gates. Production selection and settings behavior remain unchanged; LPA is separate. The integration task records the resulting commit and exact-SHA Actions outcomes on the issue.
