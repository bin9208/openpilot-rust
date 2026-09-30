# Rust runtime version helpers (#73)

- Tracking issue: https://github.com/bin9208/openpilot-rust/issues/73
- Design, source provenance, invocation recipes, compatibility boundaries and
  host/generic ARM validation: [version validation](../rust-port/version-validation.md).
- Local implementation is an isolated reusable library increment. PR/CI and
  manager integration are pending; full runtime issue #1 remains open.
- No vehicle/device connection, runtime selection change or deployment occurred.
